# GitHub repo metadata — copy-paste values

The `rosace-ui/rosace` About box was **completely empty** (checked
2026-08-13): no description, no homepage, no topics. That box is what GitHub
search ranks on and what Google shows as the result snippet, so an empty one
means the repo is effectively unfindable by anything except its own name.

Everything below is set in **Settings → General**, or via the gear icon
beside "About" on the repo home page.

---

## Description

Front-loaded with the exact phrase people search ("GUI framework for Rust"),
because GitHub search weights the description heavily and Google truncates
around 150-160 characters — the important words have to come first.

```
Declarative GUI framework for Rust — build native apps for desktop, web, iOS and Android from one codebase. GPU rendering via wgpu, fine-grained reactivity, real text input with IME, platform accessibility, and hot reload.
```

*(218 characters; GitHub's limit is 350.)*

If you would rather flag maturity in the snippet itself, append
` Early development, APIs unstable.` — it costs a little reach and buys
trust, and it matches the tone of the README's own 0.1.0 note.

## Website

```
https://rosace.godwinj.com/
```

Tick **"Use your GitHub Pages website"** off and paste this; it is the
`rosace-page` deployment, not Pages.

## Topics

GitHub allows 20. These are lowercase and hyphenated (the only format it
accepts) and each one is a topic page people actually browse:

```
rust
rust-lang
gui
gui-framework
ui-framework
rust-gui
cross-platform
declarative-ui
reactive
wgpu
gpu-rendering
webassembly
wasm
ios
android
desktop-app
hot-reload
accessibility
widgets
material-design
```

Every one is accurate. I left out `flutter`, `react` and similar
comparison bait: they pull traffic, but a topic that describes something the
project is not reads as spam and GitHub's own guidance is that topics should
describe the repository itself.

## With the `gh` CLI instead

`gh` is not installed on this machine (`command not found`), so this is for
whenever it is:

```sh
gh repo edit rosace-ui/rosace \
  --description "Declarative GUI framework for Rust — build native apps for desktop, web, iOS and Android from one codebase. GPU rendering via wgpu, fine-grained reactivity, real text input with IME, platform accessibility, and hot reload." \
  --homepage "https://rosace.godwinj.com/" \
  --add-topic rust,rust-lang,gui,gui-framework,ui-framework,rust-gui,cross-platform,declarative-ui,reactive,wgpu,gpu-rendering,webassembly,wasm,ios,android,desktop-app,hot-reload,accessibility,widgets,material-design
```

---

## Also worth doing, in rough order of payoff

1. **The README lead sentence** — done 2026-08-13. The README used to open
   with a logo and the tagline *"The UI framework Rust deserved from day
   one."* That is a slogan, not a description: nobody searches it, and
   crawlers index the first real text on the page. It now opens with a plain
   sentence naming the thing and the platforms.

2. **`rosace-examples` and `rosace-page` have empty About boxes too.**
   Suggested:
   - examples → `Example apps for the Rosace UI framework, including a 52-screen widget showcase.` · topics: `rust`, `gui`, `examples`, `rosace`
   - page → `Website for the Rosace UI framework.` · topics: `rust`, `gui`, `website`

3. **Social preview image** (Settings → General → Social preview). Without
   one, every link shared to X/Slack/Discord renders as a grey placeholder.
   `assets/rosace/rosace-logo-aurora.svg` needs rasterizing to 1280×640 PNG.

4. **crates.io keywords and categories.** `Cargo.toml` carries `description`
   but the discovery fields are separate and are what crates.io search uses:
   ```toml
   keywords = ["gui", "ui", "framework", "cross-platform", "declarative"]
   categories = ["gui", "rendering", "wasm"]
   ```
   Five keywords max, and categories must come from the fixed crates.io list.
   This only takes effect on the next publish.
