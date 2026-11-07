enum Shape {
    Strip(u32),
    Join(Box<Shape>, Box<Shape>),
}

#[derive(Debug, Eq, PartialEq)]
struct Attributes {
    width: u32,
    placements: Vec<(u32, u32)>,
}

fn place(shape: &Shape, origin: u32) -> Attributes {
    match shape {
        Shape::Strip(width) => Attributes {
            width: *width,
            placements: vec![(origin, *width)],
        },
        Shape::Join(left, right) => {
            let mut left = place(left, origin);
            let right = place(right, origin + left.width);
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
