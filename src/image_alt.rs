use std::{env::temp_dir, path::PathBuf, sync::Arc};

use async_recursion::async_recursion;
use candle_core::{
    utils::{cuda_is_available, metal_is_available},
    DType, Device, Tensor,
};
use candle_nn::VarBuilder;
use candle_transformers::{
    generation::LogitsProcessor,
    models::{moondream::Config, moondream::Model},
};
use image::{imageops::FilterType, ImageReader};
use tokenizers::Tokenizer;
use tokio::{
    fs::{remove_file, write},
    sync::Mutex,
};
use tracing::{debug, info};
use yamd::{
    nodes::{Image, Images, YamdNodes},
    Yamd,
};

use crate::{
    cache::Cache,
    error::{BarErr, ContextExt},
};

fn device() -> Result<Device, BarErr> {
    if cuda_is_available() {
        info!("CUDA is available, using CUDA device");
        Ok(Device::new_cuda(0).with_context(|| String::from("initialize cuda device"))?)
    } else if metal_is_available() {
        info!("Metal is available, using Metal device");
        Ok(Device::new_metal(0).with_context(|| String::from("initialize metal device"))?)
    } else {
        info!("No GPU available, using CPU device");
        Ok(Device::Cpu)
    }
}

async fn unwrap_path(path: &str) -> Result<(PathBuf, bool), BarErr> {
    if path.starts_with("http") {
        debug!("Downloading image from URL: {}", path);
        let response = reqwest::get(path).await?;
        if !response.status().is_success() {
            return Err(
                format!("Failed to fetch image, status code: {}", response.status()).into(),
            );
        }

        let bytes = response.bytes().await?;
        let destination = temp_dir().join(path.split('/').next_back().unwrap_or(path));
        write(&destination, bytes).await?;

        debug!("Saved image to temporary file: {:?}", destination);
        Ok((destination, true))
    } else {
        let path = PathBuf::from(path);
        if !path.exists() {
            return Err(format!("Image file does not exist: {}", path.display()).into());
        }
        Ok((path, false))
    }
}

pub struct AltGenerator {
    model: Mutex<Model>,
    device: Mutex<Device>,
    tokenizer: Tokenizer,
    bos_token: u32,
    eos_token: u32,
}

impl AltGenerator {
    pub async fn new() -> Result<Self, BarErr> {
        let api = hf_hub::api::tokio::Api::new()?;
        let repo = api.repo(hf_hub::Repo::with_revision(
            "vikhyatk/moondream1".to_string(),
            hf_hub::RepoType::Model,
            "f6e9da68e8f1b78b8f3ee10905d56826db7a5802".to_string(),
        ));
        let model_file = repo.get("model.safetensors").await?;
        let tokenizer = repo.get("tokenizer.json").await?;

        let tokenizer = Tokenizer::from_file(tokenizer)?;

        let device = device()?;
        let config = Config::v2();

        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[model_file], DType::F16, &device)? };
        let model = Model::new(&config, vb)?;

        info!("the model is initialized");

        let special_token = match tokenizer.get_vocab(true).get("<|endoftext|>") {
            Some(token) => *token,
            None => return Err("cannot find the special token".into()),
        };

        Ok(Self {
            model: Mutex::new(model),
            device: Mutex::new(device),
            tokenizer,
            bos_token: special_token,
            eos_token: special_token,
        })
    }

    async fn generate_image_alt(&self, path: &str) -> Result<String, BarErr> {
        let cache = Cache::<String>::new("image_alt", 1);
        if let Some(alt_text) = cache.get(path).await? {
            return Ok(alt_text);
        }

        let image = self.load_image(path).await?;

        info!("loaded and encoded the image {image:?}",);

        let result = self
            .run(
                "\n\nQuestion: Describe image in a short sentence.\n\nAnswer:",
                image,
            )
            .await;

        if let Ok(alt_text) = result.as_ref() {
            cache.set(path, alt_text).await?;
        }

        result
    }

    async fn yamd_image(&self, i: &Image) -> Result<Option<Image>, BarErr> {
        if !i.alt.is_empty() {
            return Ok(None);
        }
        info!("no alt text found for image: {}", i.src);
        let alt = self.generate_image_alt(i.src.as_ref()).await?;
        Ok(Some(Image {
            src: i.src.clone(),
            alt,
        }))
    }

    #[async_recursion]
    pub async fn add_alt_text(
        &self,
        (pid, yamd): (String, Yamd),
    ) -> Result<(String, Yamd), BarErr> {
        let mut nodes: Vec<YamdNodes> = Vec::with_capacity(yamd.body.len());
        for node in yamd.body.into_iter() {
            match node {
                YamdNodes::Image(image) => {
                    nodes.push(
                        self.yamd_image(&image)
                            .await?
                            .map_or_else(|| YamdNodes::Image(image), YamdNodes::Image),
                    );
                }
                YamdNodes::Images(images) => {
                    let mut new_images: Vec<Image> = Vec::with_capacity(images.body.len());
                    for image in images.body.into_iter() {
                        new_images.push(
                            self.yamd_image(&image)
                                .await?
                                .map_or_else(|| image, |new_image| new_image),
                        );
                    }
                    nodes.push(YamdNodes::Images(Images { body: new_images }));
                }
                YamdNodes::Collapsible(collapsible) => {
                    let mut new_collapsible = collapsible.clone();
                    new_collapsible.body = self
                        .add_alt_text((
                            pid.clone(),
                            Yamd::new(yamd.metadata.clone(), collapsible.body),
                        ))
                        .await?
                        .1
                        .body;
                    nodes.push(YamdNodes::Collapsible(new_collapsible));
                }
                _ => {
                    nodes.push(node.clone());
                }
            }
        }
        Ok((pid, Yamd::new(yamd.metadata.clone(), nodes)))
    }

    async fn run(&self, prompt: &str, image: Tensor) -> Result<String, BarErr> {
        let mut tokens = self.tokenizer.encode(prompt, true)?.get_ids().to_vec();

        let mut m = self.model.lock().await;
        let d = self.device.lock().await;
        debug!("locked the model and device for inference");

        m.text_model.clear_kv_cache();

        let image_embeds = image.unsqueeze(0)?.apply(m.vision_encoder())?;

        let mut logits_processor = LogitsProcessor::new(0, Some(0.1), Some(0.1));
        let mut answer = String::new();

        for index in 0..1000 {
            let context_size = if index > 0 { 1 } else { tokens.len() };
            let ctxt = &tokens[tokens.len().saturating_sub(context_size)..];
            let input = Tensor::new(ctxt, &d)?.unsqueeze(0)?;

            let logits = if index > 0 {
                m.text_model.forward(&input)?
            } else {
                let bos_token = Tensor::new(&[self.bos_token], &d)?.unsqueeze(0)?;
                m.text_model
                    .forward_with_img(&bos_token, &input, &image_embeds)?
            };

            let logits = logits.squeeze(0)?.to_dtype(DType::F32)?;
            let next_token = logits_processor.sample(&logits)?;
            tokens.push(next_token);
            answer.push_str(self.tokenizer.decode(&[next_token], true)?.as_str());

            if next_token == self.eos_token || answer.ends_with("<END>") {
                break;
            }
        }

        drop(m);
        drop(d);
        debug!("unlocked the model and device after inference");

        let result = answer
            .strip_suffix("<END>")
            .map_or_else(|| answer.trim().to_string(), |s| s.trim().to_string());

        info!("Generated alt text: {}", result);

        Ok(result)
    }

    async fn load_image(&self, path: &str) -> Result<Tensor, BarErr> {
        let (p, is_temp_file) = unwrap_path(path).await?;

        let img = ImageReader::open(&p)?
            .decode()?
            .resize_to_fill(378, 378, FilterType::Triangle);

        if is_temp_file {
            remove_file(&p).await?;
        }

        let img = img.to_rgb8();
        let data = img.into_raw();

        let d = self.device.lock().await;
        debug!("locked the device for image processing");

        let data = Tensor::from_vec(data, (378, 378, 3), &d)?.permute((2, 0, 1))?;
        let mean = Tensor::new(&[0.5f32, 0.5, 0.5], &d)?.reshape((3, 1, 1))?;
        let std = Tensor::new(&[0.5f32, 0.5, 0.5], &d)?.reshape((3, 1, 1))?;

        (data.to_dtype(DType::F32)? / 255.)?
            .broadcast_sub(&mean)?
            .broadcast_div(&std)?
            .to_device(&d)
            .with_context(|| String::from("encoding to device"))?
            .to_dtype(DType::F16)
            .with_context(|| String::from("encoding do dtype"))
    }
}

pub async fn generate_alt_text(
    (generator, pid, yamd): (Arc<AltGenerator>, String, Yamd),
) -> Result<(String, Yamd), BarErr> {
    generator.add_alt_text((pid, yamd)).await
}
