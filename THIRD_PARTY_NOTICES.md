# Third-party notices

## Sunny UI Design System

The CSS and palette/material presets in `web/src/lib/appearance/` are adapted from
[xudong7587/sunny-ui-design-system](https://github.com/xudong7587/sunny-ui-design-system),
commit `de782ec`, under GPL-3.0-only. The original full license is preserved in
`web/src/lib/appearance/LICENSE`. Generic CSS tokens are namespaced to avoid
collisions with this application's semantic colors. The host adapter and Svelte
controls integrate these styles with the existing frontend; no React dependency is added.

These files retain their original license; the upstream bili-sync license notice
is preserved separately.

## 115 protocol helpers

`crates/bili_sync/src/p115_cipher/` adapts the RSA/XOR protocol implementation from [zhifengle/rss2pan](https://github.com/zhifengle/rss2pan/tree/main/src/m115/crypto). Padding validation and encoding are adjusted for this client.

MIT License

Copyright (c) 2022 Alan Yang

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
