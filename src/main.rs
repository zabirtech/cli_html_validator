use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use clap::{Arg, Command};
use html5ever::{parse_document, ParseOpts, tendril::{StrTendril, TendrilSink}};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use markup5ever::QualName;
use colored::*;

fn main() {
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

    match validate_html_file(filename) {
        Ok(_) => println!("{}", "No validation errors found.".green()),
        Err(e) => {
            println!("{}", "HTML validation failed with errors:".red().bold());
            println!("{}", e.red());
        },
    }
}

fn validate_html_file(filename: &str) -> Result<(), String> {
    let file = File::open(filename).map_err(|_| format!("Error opening file: {}", filename))?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = Vec::new();
    buf_reader.read_to_end(&mut contents).map_err(|_| "Error reading file contents".to_string())?;

    let content_str = String::from_utf8(contents).map_err(|_| "Error converting file contents to string".to_string())?;
    let tendril = StrTendril::from_slice(&content_str);

    let bytes = tendril.as_bytes();

    let dom = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut Cursor::new(bytes))
        .map_err(|_| "Error parsing HTML document".to_string())?;

    println!("{}", "Parsed HTML document:".cyan());
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
            NodeData::Document => {
                println!("{}", "Document".bold());
            },
            NodeData::Doctype { name, .. } => {
                println!("{}: {}", "Doctype".bold(), name);
                self.validate_doctype(name);
            },
            NodeData::Element { ref name, ref attrs, .. } => {
                let attrs_vec: Vec<_> = attrs.borrow().iter()
                    .map(|attr| (attr.name.local.clone(), attr.value.clone()))
                    .collect();
                let attrs_str: Vec<_> = attrs_vec.iter()
                    .map(|(name, value)| format!("{}=\"{}\"", name, value))
                    .collect();
                println!("{}: <{} {}>", "Element".bold(), name.local, attrs_str.join(" "));

                self.context.update_context(name);
                self.validate_unique_elements(name);
                self.validate_attributes(name, &attrs_vec);
                self.validate_void_elements(name, handle);
            },
            NodeData::Text { ref contents } => {
                self.print_text(contents);
            },
            NodeData::Comment { ref contents } => {
                self.print_comment(contents);
            },
            _ => println!("{}", "Other node type".bold()),
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

    fn print_text(&self, contents: &RefCell<StrTendril>) {
        let borrowed_contents = contents.borrow();
        let trimmed_text = borrowed_contents.trim();
        if !trimmed_text.is_empty() {
            println!("{}: \"{}\"", "Text".bold(), trimmed_text);
        }
    }

    fn print_comment(&self, contents: &StrTendril) {
        println!("{}: <!--{}-->", "Comment".bold(), contents);
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

    //noinspection ALL
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
