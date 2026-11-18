//! Evaluate inherited origins and synthesized widths on a small tree.

// Syntax tree of 1D layout elements: leaves carry an intrinsic width, and
// internal Join nodes compose two subtrees left-to-right.
enum Shape {
    Strip(u32),
    Join(Box<Shape>, Box<Shape>),
}

#[derive(Debug, Eq, PartialEq)]
// Synthesized attributes: computed bottom-up from a node and its children
// and returned to the parent.
struct Attributes {
    width: u32,
    placements: Vec<(u32, u32)>,
}

// L-attributed single-pass DFS evaluator:
// - `origin` is an inherited attribute flowing top-down from the parent/left sibling.
// - Evaluating `left` before `right` maintains the invariant that `left.width` is
//   already known when computing `right`'s inherited `origin + left.width`.
fn place(shape: &Shape, origin: u32) -> Attributes {
    match shape {
        Shape::Strip(width) => Attributes {
            width: *width,
            placements: vec![(origin, *width)],
        },
        Shape::Join(left, right) => {
            let mut left = place(left, origin);
            // The right child's inherited origin depends on the left child's synthesized width.
            let right = place(right, origin + left.width);
            // Synthesize the parent's attributes by combining both children's results.
            left.width += right.width;
            left.placements.extend(right.placements);
            left
        }
    }
}

// Builds Join(Join(Strip(2), Strip(5)), Strip(4)) to exercise nested left-to-right flow.
fn sample() -> Shape {
    Shape::Join(
        Box::new(Shape::Join(
            Box::new(Shape::Strip(2)),
            Box::new(Shape::Strip(5)),
        )),
        Box::new(Shape::Strip(4)),
    )
}

fn main() {
    println!("{:?}", place(&sample(), 10));
}

#[test]
fn inherited_origin_uses_left_siblings_synthesized_width() {
    assert_eq!(
        place(&sample(), 10),
        Attributes {
            width: 11,
            placements: vec![(10, 2), (12, 5), (17, 4)],
        }
    );
}
