# Tasks for the idbd workspace and the IDBD Explorer web demo.

default:
    @just --list

# Format, lint, test and document the Rust workspace with every feature, as CI does.
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Compile the WebAssembly adapter and generate the browser glue in web/pkg.
# The pinned wasm-bindgen crate version and the installed CLI version must match.
# The story page loads three.js from web/vendor, copied from the pinned npm package.
web-build:
    cargo build --release --target wasm32-unknown-unknown -p idbd-wasm
    wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/idbd_wasm.wasm
    test -f web/node_modules/three/build/three.module.min.js || (cd web && npm ci)
    mkdir -p web/vendor
    cp web/node_modules/three/build/three.module.min.js web/vendor/
    cp web/node_modules/three/LICENSE web/vendor/three.LICENSE

# Copy only the files the pages load into _site. Tests, tooling and anything outside web/
# stay out, and the recipe fails if a path names internal.
web-site: web-build
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf _site
    mkdir -p _site/pkg _site/vendor
    cp web/index.html web/explorer.html web/styles.css _site/
    cp -R web/src _site/src
    cp -R web/story _site/story
    cp web/vendor/three.module.min.js web/vendor/three.LICENSE _site/vendor/
    cp web/pkg/idbd_wasm.js web/pkg/idbd_wasm_bg.wasm _site/pkg/
    find _site -type f | sort
    ! find _site -iname '*internal*' | grep .

# Deploy _site to Cloudflare as the Worker named in wrangler.jsonc. Needs `wrangler login`
# or CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID.
web-deploy *args: web-site
    npx --yes wrangler@4.152.0 deploy {{args}}

# Serve the story page at http://127.0.0.1:8173 and the Explorer at /explorer.html.
web-serve: web-build
    node web/serve.mjs

# Install the browser test dependencies once.
web-setup:
    cd web && npm install
    cd web && npx playwright install chromium firefox webkit

# Run the browser tests against a freshly built module.
web-test: web-build
    cd web && npm test

# Capture the demo at 390 px and 1440 px into internal/screenshots.
web-screenshots: web-build
    #!/usr/bin/env bash
    set -euo pipefail
    node web/serve.mjs & server=$!
    trap 'kill $server' EXIT
    sleep 1
    cd web && node screenshots.mjs

# Re-render the README's demo loop from the built demo. Add --hero for the hero still.
readme-assets *args: web-build
    node docs/assets/hero/render.mjs {{args}}
