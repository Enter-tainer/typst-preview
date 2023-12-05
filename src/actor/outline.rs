use std::num::NonZeroUsize;

use super::webview::CursorPosition;
use typst::foundations::{Content, NativeElement, Smart};
use typst::introspection::Introspector;
use typst::layout::Position;
use typst::model::HeadingElem;
use typst_ts_core::TypstDocument;

/// A heading in the outline panel.
#[derive(Debug, Clone)]
pub(crate) struct HeadingNode {
    element: Content,
    position: Position,
    level: NonZeroUsize,
    bookmarked: bool,
    children: Vec<HeadingNode>,
}

/// Construct the outline for the document.
pub(crate) fn get_outline(introspector: &mut Introspector) -> Option<Vec<HeadingNode>> {
    let mut tree: Vec<HeadingNode> = vec![];

    // Stores the level of the topmost skipped ancestor of the next bookmarked
    // heading. A skipped heading is a heading with 'bookmarked: false', that
    // is, it is not added to the PDF outline, and so is not in the tree.
    // Therefore, its next descendant must be added at its level, which is
    // enforced in the manner shown below.
    let mut last_skipped_level = None;
    for heading in introspector.query(&HeadingElem::elem().select()).iter() {
        let leaf = HeadingNode::leaf(introspector, (**heading).clone());

        if leaf.bookmarked {
            let mut children = &mut tree;

            // Descend the tree through the latest bookmarked heading of each
            // level until either:
            // - you reach a node whose children would be brothers of this
            // heading (=> add the current heading as a child of this node);
            // - you reach a node with no children (=> this heading probably
            // skipped a few nesting levels in Typst, or one or more ancestors
            // of this heading weren't bookmarked, so add it as a child of this
            // node, which is its deepest bookmarked ancestor);
            // - or, if the latest heading(s) was(/were) skipped
            // ('bookmarked: false'), then stop if you reach a node whose
            // children would be brothers of the latest skipped heading
            // of lowest level (=> those skipped headings would be ancestors
            // of the current heading, so add it as a 'brother' of the least
            // deep skipped ancestor among them, as those ancestors weren't
            // added to the bookmark tree, and the current heading should not
            // be mistakenly added as a descendant of a brother of that
            // ancestor.)
            //
            // That is, if you had a bookmarked heading of level N, a skipped
            // heading of level N, a skipped heading of level N + 1, and then
            // a bookmarked heading of level N + 2, that last one is bookmarked
            // as a level N heading (taking the place of its topmost skipped
            // ancestor), so that it is not mistakenly added as a descendant of
            // the previous level N heading.
            //
            // In other words, a heading can be added to the bookmark tree
            // at most as deep as its topmost skipped direct ancestor (if it
            // exists), or at most as deep as its actual nesting level in Typst
            // (not exceeding whichever is the most restrictive depth limit
            // of those two).
            while children.last().map_or(false, |last| {
                last_skipped_level.map_or(true, |l| last.level < l) && last.level < leaf.level
            }) {
                children = &mut children.last_mut().unwrap().children;
            }

            // Since this heading was bookmarked, the next heading, if it is a
            // child of this one, won't have a skipped direct ancestor (indeed,
            // this heading would be its most direct ancestor, and wasn't
            // skipped). Therefore, it can be added as a child of this one, if
            // needed, following the usual rules listed above.
            last_skipped_level = None;
            children.push(leaf);
        } else if last_skipped_level.map_or(true, |l| leaf.level < l) {
            // Only the topmost / lowest-level skipped heading matters when you
            // have consecutive skipped headings (since none of them are being
            // added to the bookmark tree), hence the condition above.
            // This ensures the next bookmarked heading will be placed
            // at most as deep as its topmost skipped ancestors. Deeper
            // ancestors do not matter as the nesting structure they create
            // won't be visible in the PDF outline.
            last_skipped_level = Some(leaf.level);
        }
    }

    (!tree.is_empty()).then_some(tree)
}

impl HeadingNode {
    fn leaf(introspector: &mut Introspector, element: Content) -> Self {
        let position = {
            let loc = element.location().unwrap();
            introspector.position(loc)
        };

        HeadingNode {
            level: element.expect_field_by_name::<NonZeroUsize>("level"),
            position,
            // 'bookmarked' set to 'auto' falls back to the value of 'outlined'.
            bookmarked: element
                .expect_field_by_name::<Smart<bool>>("bookmarked")
                .unwrap_or_else(|| element.expect_field_by_name::<bool>("outlined")),
            element,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Outline {
    items: Vec<OutlineItem>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct OutlineItem {
    title: String,
    position: Option<CursorPosition>,
    children: Vec<OutlineItem>,
}

pub fn outline(document: &TypstDocument) -> Outline {
    let mut introspector = Introspector::default();
    introspector.rebuild(&document.pages);
    let outline = crate::actor::outline::get_outline(&mut introspector);
    let mut items = Vec::with_capacity(outline.as_ref().map_or(0, Vec::len));

    for heading in outline.iter().flatten() {
        outline_item(heading, &mut items);
    }

    Outline { items }
}

fn outline_item(src: &HeadingNode, res: &mut Vec<OutlineItem>) {
    let body = src.element.expect_field_by_name::<Content>("body");
    let title = body.plain_text().trim().to_owned();

    let position = Some(CursorPosition {
        page_no: src.position.page.into(),
        x: src.position.point.x.to_pt(),
        y: src.position.point.y.to_pt(),
    });

    let mut children = Vec::with_capacity(src.children.len());
    for child in src.children.iter() {
        outline_item(child, &mut children);
    }

    res.push(OutlineItem {
        title,
        position,
        children,
    });
}
