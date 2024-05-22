use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use clap::{Arg, Command};
use html5ever::{parse_document, ParseOpts, tendril::{StrTendril, TendrilSink}};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use markup5ever::QualName;
use crossterm::{event::{self, Event, KeyCode}, execute, terminal::{self, EnterAlternateScreen, LeaveAlternateScreen}};
use tui::{backend::CrosstermBackend, Terminal};
use tui::widgets::{Block, Borders, Paragraph};
use tui::layout::{Layout, Constraint, Direction};
use tui::text::{Span, Spans};
use tui::style::{Style, Color, Modifier};
use colored::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize clap for command line arguments
    let matches = Command::new("HTML Validator")
        .version("1.0")
        .author("Your Name <your_email@example.com>")
        .about("Validates HTML files")
        .arg(Arg::new("input")
            .help("The HTML file to validate")
            .required(true)
            .index(1))
        .get_matches();

    let filename = matches.get_one::<String>("input").unwrap();

    // Setup terminal for TUI
    terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the application
    let res = run_app(&mut terminal, filename);

    // Restore terminal
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("{}: {}", "Error".red().bold(), err);
    }

    Ok(())
}

fn run_app<B: tui::backend::Backend>(terminal: &mut Terminal<B>, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Read HTML content
    let html_content = std::fs::read_to_string(filename).map_err(|_| "Error reading file contents".to_string())?;

    // Validate HTML and get result
    let result = validate_html_file(filename);

    loop {
        // Read terminal events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    return Ok(());
                }
            }
        }

        // Draw the UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.size());

            // HTML Content Box
            let html_block = Block::default().borders(Borders::ALL).title("HTML Validator");
            let html_paragraph = Paragraph::new(html_content.as_ref())
                .block(html_block)
                .wrap(tui::widgets::Wrap { trim: true });

            f.render_widget(html_paragraph, chunks[0]);

            // Result Box
            let result_block = Block::default().borders(Borders::ALL).title("Validation Results");

            let result_text = match &result {
                Ok(_) => vec![Spans::from(Span::styled("No validation errors found.", Style::default().fg(Color::Green)))],
                Err(e) => vec![Spans::from(Span::styled("HTML validation failed with errors:", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
                               Spans::from(Span::styled(e, Style::default().fg(Color::Red)))]
            };

            let result_paragraph = Paragraph::new(result_text)
                .block(result_block)
                .wrap(tui::widgets::Wrap { trim: true });

            f.render_widget(result_paragraph, chunks[1]);
        })?;
    }
}

fn validate_html_file(filename: &str) -> Result<(), String> {
    let file = File::open(filename).map_err(|_| format!("{}: {}", "Error opening file".red().bold(), filename))?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = Vec::new();
    buf_reader.read_to_end(&mut contents).map_err(|_| "Error reading file contents".red().to_string())?;

    let content_str = String::from_utf8(contents).map_err(|_| "Error converting file contents to string".red().to_string())?;
    let tendril = StrTendril::from_slice(&content_str);

    let bytes = tendril.as_bytes();

    let dom = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut Cursor::new(bytes))
        .map_err(|_| "Error parsing HTML document".red().to_string())?;

    let mut validator = HtmlValidator::new();
    validator.traverse_dom(&dom.document);

    validator.context.check_document_structure(&mut validator.errors);

    if validator.errors.is_empty() {
        Ok(())
    } else {
        Err(validator.errors.join("\n"))
    }
}

struct HtmlValidator {
    context: ValidationContext,
    errors: Vec<String>,
}

impl HtmlValidator {
    fn new() -> Self {
        Self {
            context: ValidationContext::new(),
            errors: Vec::new(),
        }
    }

    fn traverse_dom(&mut self, handle: &Handle) {
        match &handle.data {
            NodeData::Document => {},
            NodeData::Doctype { name, .. } => {
                self.validate_doctype(name);
            },
            NodeData::Element { ref name, ref attrs, .. } => {
                let attrs_vec: Vec<_> = attrs.borrow().iter()
                    .map(|attr| (attr.name.local.clone(), attr.value.clone()))
                    .collect();
                self.context.update_context(name);
                self.validate_unique_elements(name);
                self.validate_attributes(name, &attrs_vec);
                self.validate_void_elements(name, handle);
            },
            NodeData::Text { ref contents } => { let _ = contents; },
            NodeData::Comment { ref contents } => { let _ = contents; },
            _ => {},
        }

        for child in handle.children.borrow().iter() {
            self.traverse_dom(child);
        }
    }

    fn validate_doctype(&mut self, name: &str) {
        if name == "html" {
            self.context.has_doctype = true;
        } else {
            self.errors.push(format!("Invalid doctype: {}. Expected <!DOCTYPE html>.", name));
        }
    }

    fn validate_unique_elements(&mut self, name: &QualName) {
        let unique_tags = ["title", "base"];
        if unique_tags.contains(&name.local.as_ref()) {
            if !self.context.unique_elements.insert(name.local.as_ref().to_string()) {
                self.errors.push(format!("Multiple <{}> elements found. There should only be one <{}> element.", name.local, name.local));
            }
        }
    }

    fn validate_attributes(&mut self, name: &QualName, attrs_vec: &[(markup5ever::LocalName, StrTendril)]) {
        let attrs_map: HashMap<_, _> = attrs_vec.iter()
            .map(|(name, value)| (name.as_ref().to_string(), value.as_ref().to_string()))
            .collect();

        match name.local.as_ref() {
            "img" => {
                if !attrs_map.contains_key("src") {
                    self.errors.push("<img> tag is missing 'src' attribute.".to_string());
                }
                if !attrs_map.contains_key("alt") {
                    self.errors.push("<img> tag is missing 'alt' attribute.".to_string());
                }
            },
            "a" => {
                if !attrs_map.contains_key("href") {
                    self.errors.push("<a> tag is missing 'href' attribute.".to_string());
                }
            },
            _ => (),
        }
    }

    #[allow(clippy::needless_borrow)]
    fn validate_void_elements(&mut self, name: &QualName, handle: &Handle) {
        let void_elements = ["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];
        if void_elements.contains(&name.local.as_ref()) && !handle.children.borrow().is_empty() {
            self.errors.push(format!("Void element <{}> should not have children.", name.local));
        }
    }
}

struct ValidationContext {
    has_doctype: bool,
    has_html: bool,
    has_head: bool,
    has_body: bool,
    unique_elements: HashSet<String>,
}

impl ValidationContext {
    fn new() -> Self {
        Self {
            has_doctype: false,
            has_html: false,
            has_head: false,
            has_body: false,
            unique_elements: HashSet::new(),
        }
    }

    fn update_context(&mut self, name: &QualName) {
        match name.local.as_ref() {
            "html" => self.has_html = true,
            "head" => self.has_head = true,
            "body" => self.has_body = true,
            _ => (),
        }
    }

    // noinspection ALL
    fn check_document_structure(&self, errors: &mut Vec<String>) {
        if !self.has_doctype {
            errors.push("Missing <!DOCTYPE html> declaration.".to_string());
        }
        if !self.has_html {
            errors.push("Missing <html> element.".to_string());
        }
        if !self.has_head {
            errors.push("Missing <head> element.".to_string());
        }
        if !self.has_body {
            errors.push("Missing <body> element.".to_string());
        }
    }
}
