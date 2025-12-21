use hf_hub::api::tokio::ApiError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Img2TextError {
    #[error("Candle Core error: {0}")]
    CandleCore(#[from] candle_core::Error),
    #[error("Hugging Face Api Error: {0}")]
    HuggingFaceApi(#[from] ApiError),
    #[error("Tokenizer error: {0}")]
    Tokenizer(#[from] tokenizers::Error),
    #[error("Model initialization error: {0}")]
    ModelInit(String),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),
    #[error("Image Getter Error: {0}")]
    ImageGetter(String),
}
