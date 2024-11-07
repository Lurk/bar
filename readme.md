[![codecov](https://codecov.io/gh/Lurk/bar/graph/badge.svg?token=YNyVwXX7qn)](https://codecov.io/gh/Lurk/bar)

# BAR

Static web site generator

## Usage

```shell
bar <path to bar project>
```

## bar project configuration

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
# hash map to configure template free form, depends on a template
# will be provided to template
template_config:
  should_unpack_cloudinary: false
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

BAR MSRV is 1.80.0
