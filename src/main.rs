use std::fs::File;
use std::io::{BufReader, Read, Cursor};
use clap::{Arg, Command};
use html5ever::{tendril::{TendrilSink, StrTendril}, parse_document};
use html5ever::driver::ParseOpts;
use markup5ever_rcdom::{RcDom, Handle, NodeData};
use std::collections::{HashMap, HashSet};

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
        Ok(_) => println!("HTML validation completed successfully."),
        Err(e) => println!("Error: {}", e),
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

    println!("Parsed HTML document:");
    let mut errors = vec![];
    let mut context = ValidationContext::new();
    traverse_dom(&dom.document, 0, &mut errors, &mut context);

    // Post traversal checks
    context.check_document_structure(&mut errors);

    if errors.is_empty() {
        println!("No validation errors found.");
    } else {
        for error in errors {
            println!("Validation Error: {}", error);
        }
    }

    Ok(())
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

fn traverse_dom(handle: &Handle, depth: usize, errors: &mut Vec<String>, context: &mut ValidationContext) {
    let indent = " ".repeat(depth * 2);

    match &handle.data {
        NodeData::Document => {
            println!("{}Document", indent);
        },
        NodeData::Doctype { name, .. } => {
            println!("{}Doctype: {}", indent, name);
            if name.to_string() == "html" {
                context.has_doctype = true;
            } else {
                errors.push(format!("Invalid doctype: {}", name));
            }
        },
        NodeData::Element { ref name, ref attrs, .. } => {
            let attrs_vec: Vec<_> = attrs.borrow().iter()
                .map(|attr| (attr.name.local.clone(), attr.value.clone()))
                .collect();
            let attrs_str: Vec<_> = attrs_vec.iter()
                .map(|(name, value)| format!("{}=\"{}\"", name, value))
                .collect();
            println!("{}Element: <{} {}>", indent, name.local, attrs_str.join(" "));

            // Update context for document structure validation
            match name.local.as_ref() {
                "html" => context.has_html = true,
                "head" => context.has_head = true,
                "body" => context.has_body = true,
                _ => (),
            }

            // Validate unique elements
            let unique_tags = ["title", "base"];
            if unique_tags.contains(&name.local.as_ref()) {
                if !context.unique_elements.insert(name.local.as_ref().to_string()) {
                    errors.push(format!("Multiple <{}> elements found.", name.local));
                }
            }

            // Example validation: check for missing required attributes in <img> tags
            if name.local.as_ref() == "img" {
                let attrs_map: HashMap<_, _> = attrs_vec.iter()
                    .map(|(name, value)| (name.as_ref().to_string(), value.as_ref().to_string()))
                    .collect();
                if !attrs_map.contains_key("src") {
                    errors.push(format!("Missing 'src' attribute in <img> tag at depth {}", depth));
                }
                if !attrs_map.contains_key("alt") {
                    errors.push(format!("Missing 'alt' attribute in <img> tag at depth {}", depth));
                }
            }

            // Validate <a> tags
            if name.local.as_ref() == "a" {
                let attrs_map: HashMap<_, _> = attrs_vec.iter()
                    .map(|(name, value)| (name.as_ref().to_string(), value.as_ref().to_string()))
                    .collect();
                if !attrs_map.contains_key("href") {
                    errors.push(format!("Missing 'href' attribute in <a> tag at depth {}", depth));
                }
            }

            // Validate void elements
            let void_elements = ["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];
            if void_elements.contains(&name.local.as_ref()) && !handle.children.borrow().is_empty() {
                errors.push(format!("Void element <{}> should not have children at depth {}", name.local, depth));
            }
        },
        NodeData::Text { ref contents } => {
            let trimmed_text = {
                let borrowed_contents = contents.borrow();
                borrowed_contents.trim().to_owned()
            };
            if !trimmed_text.is_empty() {
                println!("{}Text: \"{}\"", indent, trimmed_text);
            }
        },
        NodeData::Comment { ref contents } => {
            println!("{}Comment: <!--{}-->", indent, contents);
        },
        _ => println!("{}Other node type", indent),
    }

    for child in handle.children.borrow().iter() {
        traverse_dom(child, depth + 1, errors, context);
    }
}
