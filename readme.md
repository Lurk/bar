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
content_path: './content/' # path to folder that contain yamd files
template: '../hamon/' # path to template
domain: 'https://blog.com' 
title: 'this is the blog'
description: 'blog'
dist_path: './dist' # destination
robots_txt: './public/robots.txt' # Optional. Check 'robots.txt' part in this document for more info
template_config: # hash map to configure template free form, depends on a template 
  favicon: './public/favicon.ico'
  svg_icon: './public/icon.svg'
  apple_touch_icon: './public/icon.png'
  webmanifest: './public/site.webmanifest'
```

### robots.txt

If present will be copied to the destination folder.

If not present default `robots.txt` will be generated:

```text
User-agent: *
Allow: /
```


## Templates

bar uses [Tera](https://crates.io/crates/tera) templating engine. 

Example of bar template: [Hamon](https://github.com/Lurk/Hamon)

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




## Minimal Rust Version

BAR MSRV is 1.70.0
