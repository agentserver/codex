//! Block-level partitioning for streaming markdown previews.
//!
//! Completed source is split into two regions: every top-level block before
//! the last block is stable, while the final block remains mutable until a
//! later block appears or the stream finalizes.

use std::ops::Range;

use pulldown_cmark::Event;
use pulldown_cmark::Tag;
use pulldown_cmark::TagEnd;

use crate::markdown_render::markdown_parser;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MarkdownStreamPartition {
    pub(super) stable_end: usize,
}

pub(super) fn partition_completed_source(source: &str) -> MarkdownStreamPartition {
    let blocks = top_level_block_ranges(source);
    let stable_end = blocks
        .iter()
        .rev()
        .nth(1)
        .map_or(0, |previous_block| previous_block.end);
    MarkdownStreamPartition { stable_end }
}

fn top_level_block_ranges(source: &str) -> Vec<Range<usize>> {
    let mut blocks = Vec::new();
    let mut block_start = None;
    let mut depth = 0usize;

    for (event, range) in markdown_parser(source).into_offset_iter() {
        match event {
            Event::Start(tag) if is_block_start(&tag) => {
                if depth == 0 {
                    block_start = Some(range.start);
                }
                depth += 1;
            }
            Event::End(tag) if is_block_end(tag) => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0
                    && let Some(start) = block_start.take()
                {
                    blocks.push(start..range.end);
                }
            }
            Event::Rule if depth == 0 => blocks.push(range),
            _ => {}
        }
    }

    blocks
}

fn is_block_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::FootnoteDefinition(_)
            | Tag::Table(_)
            | Tag::MetadataBlock(_)
    )
}

fn is_block_end(tag: TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::BlockQuote
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::List(_)
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
            | TagEnd::MetadataBlock(_)
    )
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::partition_completed_source;

    #[test]
    fn one_block_has_no_stable_prefix() {
        assert_eq!(partition_completed_source("hello\n"), Default::default());
    }

    #[test]
    fn second_top_level_block_stabilizes_the_first() {
        assert_eq!(
            partition_completed_source("first\n\nsecond\n").stable_end,
            "first\n".len(),
        );
    }

    #[test]
    fn nested_blocks_do_not_split_the_tail() {
        assert_eq!(
            partition_completed_source("> quoted\n>\n> still quoted\n").stable_end,
            0,
        );
    }

    #[test]
    fn rule_counts_as_a_top_level_block() {
        assert_eq!(
            partition_completed_source("before\n\n---\n\nafter\n").stable_end,
            "before\n\n---\n".len(),
        );
    }

    #[test]
    fn table_remains_mutable_until_a_later_block_arrives() {
        assert_eq!(
            partition_completed_source("| A | B |\n| --- | --- |\n| 1 | 2 |\n").stable_end,
            0,
        );
        assert_eq!(
            partition_completed_source("| A | B |\n| --- | --- |\n| 1 | 2 |\n\nnext\n").stable_end,
            "| A | B |\n| --- | --- |\n| 1 | 2 |\n".len(),
        );
    }

    #[test]
    fn fenced_table_text_stays_inside_code_block() {
        assert_eq!(
            partition_completed_source("```\n| A | B |\n| --- | --- |\n```\n").stable_end,
            0,
        );
    }
}
