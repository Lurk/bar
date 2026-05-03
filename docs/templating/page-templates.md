# Page Templates

Bar uses [Tera](https://keats.github.io/tera/) for page templates. All `.html` files in the theme directory are loaded as Tera templates and can reference each other via Tera's standard `extends`, `include`, and `block` directives.

## Template inheritance

The conventional pattern is a single `base.html` that defines named blocks, with page templates extending it:

**`base.html`**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{% block head %}{{ title }} — {{ config.title }}{% endblock head %}</title>
  <meta name="description" content="{{ description }}">
  <meta property="og:type" content="{% block og_type %}website{% endblock og_type %}">
  <meta property="og:url" content="{% block og_url %}{{ config.domain }}{% endblock og_url %}">
  {% block og_image %}{% endblock og_image %}
  <style>{{ fragment_styles }}</style>
</head>
<body>
  <nav>{% block nav %}{% endblock nav %}</nav>
  <main>{% block content %}{% endblock content %}</main>
</body>
</html>
```

**`article.html`**

```html
{% extends "base.html" %}

{% block og_type %}article{% endblock og_type %}
{% block og_url %}{{ config.domain }}{{ path }}{% endblock og_url %}

{% block content %}
  <article>{{ rendered_body | safe }}</article>
{% endblock content %}
```

Common block names used in the pattern above: `content`, `nav`, `head`, `og_type`, `og_url`, `og_image`. These are conventions, not enforced by bar — name your blocks however you like.

## Context variables

These variables are available in every page template render:

| Variable | Type | Description |
|----------|------|-------------|
| `config` | object | Site configuration from `config.yaml` |
| `config.domain` | URL | Site root URL (e.g. `https://example.com/`) |
| `config.title` | string | Site title |
| `config.description` | string | Site description |
| `config.template_config` | map | Arbitrary key/value pairs from `config.yaml` |
| `title` | string | Page title |
| `description` | string | Page description |
| `path` | string | URL path for this page (e.g. `/posts/hello`) |
| `page_num` | integer | Pagination offset (0 for the first page) |
| `fragment_styles` | string | Concatenated CSS for all YAMD node types used on this page |
| `rendered_body` | string | Pre-rendered HTML from YAMD content (use `\| safe` to avoid escaping) |

`fragment_styles` and `rendered_body` are only non-empty when the page path matches a content file in `content_path`.

## Custom Tera functions

These functions are registered by bar and available in all templates.

### `add_page(path, template, title, description, page_num?)`

Registers a dynamic page to be rendered. Bar's render loop continues until no unrendered pages remain, so calling this from one template can queue another.

```html
{{ add_page(path="/posts/hello.html", template="article.html", title="Hello", description="My first post") }}
```

| Arg | Default | Description |
|-----|---------|-------------|
| `path` | `/` | URL path for the page |
| `template` | `index.html` | Template file to render |
| `title` | `""` | Page title |
| `description` | `""` | Page description |
| `page_num` | `0` | Pagination page number |

### `add_feed(path, type)`

Registers a feed to be generated. Both arguments are required.

```html
{{ add_feed(path="/feed.json", type="json") }}
{{ add_feed(path="/feed.xml", type="atom") }}
```

Valid `type` values: `json`, `atom`.

### `add_static_file(path, source?)`

Registers a static file to be copied to dist. Returns `path`.

```html
{% set css = add_static_file(path="/style.css") %}
{% set icon = add_static_file(path="/icon.png", source="/assets/favicon.png") %}
```

When `source` is omitted, bar copies the file at `path` (relative to the project root). When `source` is provided, that file is copied to the destination `path` in dist.

### `get_static_file(path)`

Returns `path` with a cache-busting query parameter derived from the file's contents. The file must have been registered with `add_static_file` first.

```html
<link rel="stylesheet" href="{{ get_static_file(path='/style.css') }}">
{# renders as: /style.css?cb=AbCdEfGh #}
```

### `get_page_by_path(path)`

Returns the content page at the given URL path, or null if not found. The `.html` extension is stripped automatically.

```html
{% set page = get_page_by_path(path="/posts/hello.html") %}
```

### `get_page_by_pid(pid)`

Returns the content page with the given PID (path without extension, relative to `content_path`).

```html
{% set page = get_page_by_pid(pid="/posts/hello") %}
```

### `get_pages_by_tag(tag, limit?, offset?)`

Returns a paginated slice of content pages tagged with `tag`. Errors if the tag does not exist.

```html
{% set result = get_pages_by_tag(tag="rust", limit=10, offset=0) %}
{% for page in result.pages %}
  <a href="{{ page.pid }}.html">{{ page.metadata.title }}</a>
{% endfor %}
```

| Arg | Default | Description |
|-----|---------|-------------|
| `tag` | `""` | Tag to filter by |
| `limit` | `3` | Max pages to return |
| `offset` | `0` | Skip this many pages (use `page_num * limit` for pagination) |

The returned object has: `pages` (array), `current_slice`, `total_slices`, `slice_size`, `numbers` (array of `{number, display, is_current}`).

### `get_similar(pid, limit?)`

Returns an array of PIDs for pages that share the most tags with the given PID.

```html
{% set similar = get_similar(pid=page.pid, limit=3) %}
```

| Arg | Default |
|-----|---------|
| `limit` | `3` |

### `get_image_url(src, width?, height?, ar_width?, ar_height?)`

Returns a transformed image URL.

- For local paths (starting with `/`): registers the file as a static asset and returns the path unchanged.
- For Cloudinary URLs: applies a transformation and returns the transformed URL.
- For other URLs: returns the URL unchanged.

At least one of `width` or `height` is required for Cloudinary transformations.

```html
{{ get_image_url(src=page.metadata.image, width=800) }}
{{ get_image_url(src=page.metadata.image, width=400, height=300) }}
{{ get_image_url(src=page.metadata.image, height=600, ar_width=16, ar_height=9) }}
```

### `render_gpx(input, width?, height?)`

Renders a GPX file to a PNG map image, registers it as a static file, and returns the image path. Requires GPX embedding to be configured in `config.yaml`.

```html
<img src="{{ render_gpx(input='/tracks/ride.gpx', width=800, height=400) }}">
```

| Arg | Default |
|-----|---------|
| `width` | `800` |
| `height` | `600` |

### `get_gpx_stats(input)`

Returns distance and elevation statistics for a GPX file.

```html
{% set stats = get_gpx_stats(input='/tracks/ride.gpx') %}
Distance: {{ stats.distance_km }} km
```

## Custom filters

### `crc32`

Hashes a string using SeaHash and returns a URL-safe base64 string. Useful for generating stable identifiers.

```html
{{ page.pid | crc32 }}
```
