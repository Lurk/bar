# img2text

Uses Huggingface's API to get [vikhyatk/moondream1](https://huggingface.co/vikhyatk/moondream1) model (~3.7 GB). Works
on CPU or Metal if available. Heavily inspired by this
[example](https://github.com/huggingface/candle/tree/main/candle-examples/examples/moondream) from
[Candle](https://github.com/huggingface/candle) repository.

Initialization is quite expensive, especially on first run. Pass as many images as possible to amortize the cost.

## Usage as a CLI

```bash
Usage: img2text [OPTIONS] --prompt <PROMPT>

Options:
  -s, --source <SOURCE>            The path or URL to the image, can be specified multiple times
  -p, --prompt <PROMPT>            The prompt to generate alt text
  -t, --temperature <TEMPERATURE>  The temperature for generation
  -h, --help                       Print help
```

## Minimal Supported Rust Version

img2text MSRV is 1.88.0

