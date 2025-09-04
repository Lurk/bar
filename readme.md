[![codecov](https://codecov.io/gh/Lurk/bar/graph/badge.svg?token=YNyVwXX7qn)](https://codecov.io/gh/Lurk/bar)

# BAR

Static web site generator.

## Usage

### Build BAR project. [default]

```shell
Usage: bar build [OPTIONS] [PATH]

Arguments:
  [PATH]  Path to the project directory [default: .]

Options:
  -v, --verbose...  Increase logging verbosity
  -q, --quiet...    Decrease logging verbosity
  -h, --help        Print help
```

### Create a new article in the current directory.

```shell
Usage: bar article [OPTIONS] <TITLE>

Arguments:
  <TITLE>  Title of the article will be used as the file name

Options:
  -f, --force       By default BAR will fail if article with the same title already exists. Use this flag to overwrite exiting one
  -v, --verbose...  Increase logging verbosity
  -q, --quiet...    Decrease logging verbosity
  -h, --help        Print help
```

### Clear dist and cache directory.

```shell
Usage: bar clear [OPTIONS] [PATH]

Arguments:
  [PATH]  Path to the project directory [default: .]

Options:
  -v, --verbose...  Increase logging verbosity
  -q, --quiet...    Decrease logging verbosity
  -h, --help        Print help
```

## BAR project configuration

Configuration file `config.yaml` should be in root folder of a project.

```yaml
# destination
dist_path: './dist'
# path to folder that contain yamd files
content_path: './content/'
#path from where get static files
static_source_path: './static'
# Non required filed with white list of allowed file extensions.
# Defaults to ["css", "js", "png", "jpg", "jpeg", "gif", "svg", "webmanifest", "ico", "txt"]
static_files_extensions:
  - txt
  - jpg
  - png
# path to template
template: '../hamon/'
domain: 'https://blog.com'
title: 'this is the blog'
description: 'blog'
yamd_processors:
  # if set to true BAR will convert Cloudinary [Embed](https://docs.rs/yamd/latest/yamd/nodes/struct.Embed.html)
  # to [Images](https://docs.rs/yamd/latest/yamd/nodes/struct.Images.html)
  convert_cloudinary_embed: true
  # If set BAR will generate alt text for images using
  # [MoonDream1](https://huggingface.co/vikhyatk/moondream1) model locally. It will do so only for images that do not
  # have alt text.
  #
  # Generation takes seconds per image and depends on prompt.
  #
  # Remote images will be downloaded and cached in `.cache/remote_images/` directory.
  #
  # Result will be cached in `.cache/alt_text/` directory.
  #
  # Disabled by default.
  #
  # Caution:
  # - It will download [MoonDream1 model](https://huggingface.co/vikhyatk/moondream1) from HuggingFace (3.72GB).
  # - it was tested only on Apple M2 Max.
  generate_alt_text:
    # Prompt to use for alt text generation.
    prompt: 'Describe image in one sentence.'
    # temperature for alt text generation.
    temperature: 0.1
# HashMap to configure template (depends on a template)
# Supported types:
# - Boolean (bool),
# - Integer (usize),
# - String (String),
# - Vector Of Strings (Vec<String>),
# - Map Of String To String (LinkedHashMap<String, String>) preserves order,
# - Map Of String To Map Of String To String(LinkedHashMap<String, LinkedHashMap<String, String>>) preserves order,
#
# will be provided to template as `config.template_config`
template_config:
  # Example of boolean config
  show_rss: true
  # Example of integer config
  articles_per_page: 5
  # Example of string config
  value: 'string value'
  # Example of vector of strings config
  authors:
    - 'author 1'
    - 'author 2'
  # Example of map of string to string config
  social:
    mastodon: 'https://mastodon.social/@user'
  # Example of map of string to map of string to string config
  analytics:
    plausible:
      domain: 'blog.com'
    umami:
      website_id: 'your-website-id'
```

## Static files

BAR will gather static files from:

1. Path specified in `config.static_source_path`
2. `static` directory in template
3. Defaults from BAR (check out [robots.txt](#robotstxt) section)

If file exists source it will be not overwritten with file from template or BAR. If however you want to overwrite file
from template with your own version. For example if you want custom CSS, you can add it to the static folder of source.

Only files with allowed extensions will be copied. Default list of extensions:

- css
- js
- png
- jpg
- jpeg
- gif
- svg
- webmanifest
- ico
- txt

It can be customized with `config.static_files_extensions` param.

## robots.txt

If source or template does not provide the `robots.txt` static file, BAR will generate default one:

```text
User-agent: *
Allow: /
```

## Templates

BAR uses [Tera](https://crates.io/crates/tera) templating engine.

Example of bar template: [Hamon](https://github.com/Lurk/Hamon)

### 404 page

If bar template has `./404.html` it will be rendered to the `config.dest_path` + `404.html`

### Functions

#### add_page

Takes 5 arguments

- path
- template
- title
- description
- page_num

Example:

```htmldjango
{{ add_page(path = '/',template = 'index.html', title = config.title, description = config.description, page_num = 0) }}

```

#### get_static_file

Takes one argument

- path

Returns relative url with cache buster. Cache buster is crc32 of a file content.

example:

```htmldjango
{{ get_static_file( path='/favicon.ico' )}}
```

## Minimal Rust Version

BAR MSRV is 1.85.0
