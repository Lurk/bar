pub mod error;

use std::{path::PathBuf, sync::Arc};

use candle_core::{DType, Device, Tensor, utils::metal_is_available};
use candle_nn::VarBuilder;
use candle_transformers::{
    generation::LogitsProcessor,
    models::{moondream::Config, moondream::Model},
};
use image::{ImageReader, imageops::FilterType};
use tokenizers::Tokenizer;
use tokio::sync::{Mutex, OnceCell};
use tracing::{debug, info};

use crate::error::Img2TextError;

fn device() -> Result<Device, Img2TextError> {
    if metal_is_available() {
        info!("Metal is available, using Metal device");
        Ok(Device::new_metal(0)?)
    } else {
        info!("No GPU available, using CPU device");
        Ok(Device::Cpu)
    }
}

struct Img2TextInner {
    model: Model,
    device: Device,
    tokenizer: Tokenizer,
    bos_token: u32,
    eos_token: u32,
}

impl Img2TextInner {
    pub async fn new() -> Result<Self, Img2TextError> {
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

        // # Safety:
        //
        // Nothing guarantees that the safetensors file is not updated afterwards. For more info look at
        // [`memmap2::MmapOptions`]
        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[model_file], DType::F16, &device)? };
        let model = Model::new(&config, vb)?;

        info!("vikhyatk/moondream1 is initialized");

        let special_token = match tokenizer.get_vocab(true).get("<|endoftext|>") {
            Some(token) => *token,
            None => {
                return Err(Img2TextError::ModelInit(
                    "cannot find the special token".into(),
                ));
            }
        };

        Ok(Self {
            model,
            device,
            tokenizer,
            bos_token: special_token,
            eos_token: special_token,
        })
    }
}

pub struct Img2Text {
    inner: Mutex<OnceCell<Img2TextInner>>,
}

impl Default for Img2Text {
    fn default() -> Self {
        Self::new()
    }
}

impl Img2Text {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(OnceCell::const_new()),
        }
    }

    pub async fn run(
        &self,
        path: &PathBuf,
        prompt: &str,
        temperature: f64,
    ) -> Result<Arc<str>, Img2TextError> {
        let image = self.load_image(path).await?;

        debug!("model and device are locked for inference");
        let mut inner_lock = self.inner.lock().await;

        inner_lock
            .get_or_try_init(|| async { Img2TextInner::new().await })
            .await?;
        let inner = inner_lock.get_mut().expect("Inner should be initialized");
        let mut tokens = inner
            .tokenizer
            .encode(format!("\n\nQuestion: {prompt}\n\nAnswer:"), true)?
            .get_ids()
            .to_vec();

        inner.model.text_model.clear_kv_cache();

        let image_embeds = image.unsqueeze(0)?.apply(inner.model.vision_encoder())?;

        let mut logits_processor = LogitsProcessor::new(0, Some(temperature), Some(0.1));
        let mut answer = String::new();

        for index in 0..1000 {
            let context_size = if index > 0 { 1 } else { tokens.len() };
            let ctxt = &tokens[tokens.len().saturating_sub(context_size)..];
            let input = Tensor::new(ctxt, &inner.device)?.unsqueeze(0)?;

            let logits = if index > 0 {
                inner.model.text_model.forward(&input)?
            } else {
                let bos_token = Tensor::new(&[inner.bos_token], &inner.device)?.unsqueeze(0)?;
                inner
                    .model
                    .text_model
                    .forward_with_img(&bos_token, &input, &image_embeds)?
            };

            let logits = logits.squeeze(0)?.to_dtype(DType::F32)?;
            let next_token = logits_processor.sample(&logits)?;
            tokens.push(next_token);
            answer.push_str(inner.tokenizer.decode(&[next_token], true)?.as_str());

            if next_token == inner.eos_token || answer.ends_with("<END>") {
                break;
            }
        }

        let result = answer
            .strip_suffix("<END>")
            .map_or_else(|| Arc::from(answer.trim()), |s| Arc::from(s.trim()));

        info!("Generated alt text: {}", result);

        Ok(result)
    }

    async fn load_image(&self, path: &PathBuf) -> Result<Tensor, Img2TextError> {
        let img = ImageReader::open(path)?
            .decode()?
            .resize_to_fill(378, 378, FilterType::Triangle);

        let img = img.to_rgb8();
        let data = img.into_raw();

        let inner_lock = self.inner.lock().await;

        let inner = inner_lock
            .get_or_try_init(|| async { Img2TextInner::new().await })
            .await?;
        debug!("locked the device for image processing");

        let data = Tensor::from_vec(data, (378, 378, 3), &inner.device)?.permute((2, 0, 1))?;
        let mean = Tensor::new(&[0.5f32, 0.5, 0.5], &inner.device)?.reshape((3, 1, 1))?;
        let std = Tensor::new(&[0.5f32, 0.5, 0.5], &inner.device)?.reshape((3, 1, 1))?;

        Ok((data.to_dtype(DType::F32)? / 255.)?
            .broadcast_sub(&mean)?
            .broadcast_div(&std)?
            .to_device(&inner.device)?
            .to_dtype(DType::F16)?)
    }
}
