# Code-Forge Documentation

This folder contains the documentation for Code-Forge, using the Just the Docs Jekyll theme.

## Running the Documentation Locally

### Prerequisites

- Ruby installed (version 2.7.0 or higher recommended)
- Bundler gem installed (`gem install bundler`)

### Setup and Run

1. Navigate to the docs directory:
   ```bash
   cd docs
   ```

2. Install dependencies:
   ```bash
   bundle install
   ```

3. Start the local server:
   ```bash
   bundle exec jekyll serve
   ```

4. Open your browser and go to: `http://localhost:4000`

## Adding or Modifying Documentation

1. All documentation is written in Markdown (.md) files
2. Each file should have front matter at the top like this:
   ```yaml
   ---
   layout: page
   title: Page Title
   nav_order: 2
   description: "Description of the page"
   permalink: /page-url
   ---
   ```

3. The `nav_order` value determines the position in the navigation menu

## Documentation Structure

- `index.md`: Home page
- `onboarding.md`: Onboarding guide
- `guidelines.md`: Development guidelines
- `service.md`: Service documentation
- `_config.yml`: Jekyll configuration
- `Gemfile`: Ruby gem dependencies

## Theme Configuration

The Just the Docs theme is configured in `_config.yml`. Refer to the [Just the Docs documentation](https://just-the-docs.github.io/just-the-docs/) for customization options.