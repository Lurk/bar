use yamd::{
    nodes::{Image, Images},
    op::{Content, Node, Op},
};

fn image_to_ops(image: &Image) -> Vec<Op> {
    vec![
        Op::new_start(Node::Image, Content::Span(0..0)),
        Op::new_start(Node::Title, Content::Span(0..0)),
        Op::new_value(Content::Materialized(image.alt.clone())),
        Op::new_end(Node::Title, Content::Span(0..0)),
        Op::new_start(Node::Destination, Content::Span(0..0)),
        Op::new_value(Content::Materialized(image.src.clone())),
        Op::new_end(Node::Destination, Content::Span(0..0)),
        Op::new_end(Node::Image, Content::Span(0..0)),
    ]
}

pub(crate) fn images_to_ops(images: &Images) -> Vec<Op> {
    let mut ops = vec![Op::new_start(Node::Images, Content::Span(0..0))];
    for image in &images.body {
        ops.extend(image_to_ops(image));
    }
    ops.push(Op::new_end(Node::Images, Content::Span(0..0)));
    ops
}
