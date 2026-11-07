//! Evaluate inherited origins and synthesized widths on a small tree.

enum Shape {
    Strip(u32),
    Join(Box<Shape>, Box<Shape>),
}

#[derive(Debug, Eq, PartialEq)]
// These attributes travel upward as the result of placing a subtree.
struct Attributes {
    width: u32,
    placements: Vec<(u32, u32)>,
}

// The origin is inherited by this node from its context.
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
            // Combine both children's synthesized results for the parent.
            left.width += right.width;
            left.placements.extend(right.placements);
            left
        }
    }
}

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
