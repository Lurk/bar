use async_recursion::async_recursion;
use cloudinary::{tags::get_tags, transformation::Image as CloudinaryImage};
use numeric_sort::cmp;
use yamd::{
    nodes::{Collapsible, Embed, Image, Images, YamdNodes},
    Yamd,
};

use crate::{cache::Cache, error::BarErr};

async fn cloudinary_gallery_to_image_gallery(embed: &Embed) -> Result<Images, BarErr> {
    let cache = Cache::<Images>::new("cloudinary_gallery", 1);

    if let Some(images) = cache.get(&embed.args).await? {
        return Ok(images);
    }

    if let Some((cloud_name, tag)) = embed.args.split_once('&') {
        let mut tags = get_tags(cloud_name.into(), tag.into())
            .await
            .unwrap_or_else(|_| panic!("error loading cloudinary tag: {}", tag));

        tags.resources
            .sort_by(|a, b| cmp(&a.public_id, &b.public_id));

        let images = tags
            .resources
            .iter()
            .map(|resource| {
                let mut image = CloudinaryImage::new(cloud_name.into(), resource.public_id.clone());
                image.set_format(resource.format.as_ref());
                Image::new(resource.public_id.to_string(), image.to_string())
            })
            .collect::<Vec<Image>>();
        let images = Images::new(images);
        cache.set(embed.args.as_str(), &images).await?;
        return Ok(images);
    }
    Err(
        "cloudinary_gallery embed must have two arguments: cloud_name and tag separated by '&'."
            .into(),
    )
}

#[async_recursion]
async fn process_collapsible(collapsible: &Collapsible) -> Result<Collapsible, BarErr> {
    let mut nodes_vec: Vec<YamdNodes> = Vec::with_capacity(collapsible.body.len());
    for node in collapsible.body.iter() {
        match node {
            YamdNodes::Embed(embed) if embed.kind == "cloudinary_gallery" => {
                nodes_vec.push(cloudinary_gallery_to_image_gallery(embed).await?.into());
            }
            YamdNodes::Collapsible(collapsible) => {
                nodes_vec.push(process_collapsible(collapsible).await?.into());
            }
            _ => nodes_vec.push(node.clone()),
        }
    }
    Ok(Collapsible::new(collapsible.title.clone(), nodes_vec))
}

pub async fn unwrap_cloudinary((pid, yamd): (String, Yamd)) -> Result<(String, Yamd), BarErr> {
    let mut nodes: Vec<YamdNodes> = Vec::with_capacity(yamd.body.len());
    for node in yamd.body.iter() {
        match node {
            YamdNodes::Embed(embed) if embed.kind == "cloudinary_gallery" => {
                nodes.push(cloudinary_gallery_to_image_gallery(embed).await?.into());
            }
            YamdNodes::Collapsible(collapsible) => {
                nodes.push(process_collapsible(collapsible).await?.into());
            }
            _ => nodes.push(node.clone()),
        }
    }
    Ok((pid, Yamd::new(yamd.metadata.clone(), nodes)))
}
