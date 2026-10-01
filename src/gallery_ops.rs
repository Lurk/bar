use yamd::{
    nodes::{Image, Images},
    op::{Content, Node, Op},
};

fn image_to_ops(image: &Image) -> Vec<Op> {
    vec![
        Op::new_start(Node::Image, Content::empty()),
        Op::new_start(Node::Title, Content::empty()),
        Op::new_value(Content::detached(image.alt.clone())),
        Op::new_end(Node::Title, Content::empty()),
        Op::new_start(Node::Destination, Content::empty()),
        Op::new_value(Content::detached(image.src.clone())),
        Op::new_end(Node::Destination, Content::empty()),
        Op::new_end(Node::Image, Content::empty()),
    ]
}

pub(crate) fn images_to_ops(images: &Images) -> Vec<Op> {
    let mut ops = vec![Op::new_start(Node::Images, Content::empty())];
    for image in &images.body {
        ops.extend(image_to_ops(image));
    }
    ops.push(Op::new_end(Node::Images, Content::empty()));
    ops
}
