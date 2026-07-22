use comrak::nodes::{AstNode, ListType, NodeValue};

use super::inline::inlines_to_typst;
use crate::error::Result;

pub(super) fn block_to_typst<'a>(node: &'a AstNode<'a>, out: &mut String) -> Result<()> {
    match node.data.borrow().value.clone() {
        NodeValue::Heading(heading) => {
            out.push_str(&"=".repeat(heading.level as usize));
            out.push(' ');
            out.push_str(&inlines_to_typst(node)?);
            out.push_str("\n\n");
        }
        NodeValue::Paragraph => {
            out.push_str(&inlines_to_typst(node)?);
            out.push_str("\n\n");
        }
        NodeValue::List(list) => {
            let marker = match list.list_type {
                ListType::Bullet => "- ",
                ListType::Ordered => "+ ",
            };
            for item in node.children() {
                out.push_str(marker);
                let mut body = String::new();
                for child in item.children() {
                    block_to_typst(child, &mut body)?;
                }
                out.push_str(body.trim_end());
                out.push('\n');
            }
            out.push('\n');
        }
        NodeValue::CodeBlock(code) => {
            out.push_str("```");
            out.push_str(&code.info);
            out.push('\n');
            out.push_str(&code.literal);
            if !code.literal.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n\n");
        }
        NodeValue::Table(table) => {
            out.push_str("#table(\n  columns: ");
            out.push_str(&table.num_columns.to_string());
            out.push(',');
            for row in node.children() {
                out.push_str("\n ");
                for cell in row.children() {
                    out.push_str(" [");
                    out.push_str(&inlines_to_typst(cell)?);
                    out.push_str("],");
                }
            }
            out.push_str("\n)\n\n");
        }
        NodeValue::BlockQuote => {
            let mut body = String::new();
            for child in node.children() {
                block_to_typst(child, &mut body)?;
            }
            out.push_str("#quote(block: true)[\n");
            out.push_str(body.trim_end());
            out.push_str("\n]\n\n");
        }
        NodeValue::ThematicBreak => out.push_str("#line(length: 100%)\n\n"),
        NodeValue::HtmlBlock(html) => {
            out.push_str(&html.literal);
            out.push('\n');
        }
        _ => {
            out.push_str(&inlines_to_typst(node)?);
            out.push_str("\n\n");
        }
    }
    Ok(())
}
