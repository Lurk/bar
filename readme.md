# bar

Static web site generator

## Usage

```shell
bar <path to bar project>
```

## bar project configuration

Configuration file `config.toml` should be in root folder of a project.

```toml
content_path: './content/' # path to folder that contain yamd files
template: '../hamon/' # path to template
domain: 'https://blog.com' 
title: 'this is the blog'
description: 'blog'
dist_path: './dist' # destination
template_config: # hash map to configure template free form, depends on a template 
  favicon: './public/favicon.ico'
  svg_icon: './public/icon.svg'
  apple_touch_icon: './public/icon.png'
  webmanifest: './public/site.webmanifest'
```

## Templates

bar uses [Tera](https://crates.io/crates/tera) templating engine.

### Functions

#### add_page

Takes 5 arguments

- path
- template
- title
- description
- page_num

example:

```htmldjango
{{ add_page(path = '/',template = 'index.html', title = config.title, description = config.description, page_num = 0) }}

```

