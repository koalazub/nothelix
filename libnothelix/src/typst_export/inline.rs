use comrak::nodes::{AstNode, NodeValue};

use super::math::latex_to_typst_math;
use crate::error::Result;

pub(super) fn inlines_to_typst<'a>(node: &'a AstNode<'a>) -> Result<String> {
    let mut out = String::new();
    for child in node.children() {
        inline_to_typst(child, &mut out)?;
    }
    Ok(out)
}

fn inline_to_typst<'a>(node: &'a AstNode<'a>, out: &mut String) -> Result<()> {
    match node.data.borrow().value.clone() {
        NodeValue::Text(text) => out.push_str(&escape_typst(&text)),
        NodeValue::SoftBreak => out.push(' '),
        NodeValue::LineBreak => out.push_str("\\ "),
        NodeValue::Strong => {
            out.push('*');
            out.push_str(&inlines_to_typst(node)?);
            out.push('*');
        }
        NodeValue::Emph => {
            out.push('_');
            out.push_str(&inlines_to_typst(node)?);
            out.push('_');
        }
        NodeValue::Strikethrough => {
            out.push_str("#strike[");
            out.push_str(&inlines_to_typst(node)?);
            out.push(']');
        }
        NodeValue::Code(code) => {
            out.push('`');
            out.push_str(&code.literal);
            out.push('`');
        }
        NodeValue::Math(math) => {
            let converted = latex_to_typst_math(&math.literal)?;
            if math.display_math {
                out.push_str("$ ");
                out.push_str(&converted);
                out.push_str(" $");
            } else {
                out.push('$');
                out.push_str(&converted);
                out.push('$');
            }
        }
        NodeValue::Link(link) => {
            out.push_str("#link(\"");
            out.push_str(&link.url);
            out.push_str("\")[");
            out.push_str(&inlines_to_typst(node)?);
            out.push(']');
        }
        _ => out.push_str(&inlines_to_typst(node)?),
    }
    Ok(())
}

fn escape_typst(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '#' | '$' | '[' | ']' | '*' | '_' | '@' | '<' | '>'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}
