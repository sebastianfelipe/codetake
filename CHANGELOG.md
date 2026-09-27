# Changelog

## 0.1.0 (2026-09-27)


### Features

* **app:** add export and media playback commands ([dc57f97](https://github.com/sebastianfelipe/codetake/commit/dc57f97a7c1319b89dae03fbf6060791f3a1a336))
* **app:** expose recording to the UI through Tauri commands ([d613bdc](https://github.com/sebastianfelipe/codetake/commit/d613bdcb8c3b090dc18e531325998da005c0d32f))
* **audio:** add audio mixer and format conversion ([328b471](https://github.com/sebastianfelipe/codetake/commit/328b4710177df9f421611cb7e21e009bebc58ddf))
* **audio:** add background music ([325e7bb](https://github.com/sebastianfelipe/codetake/commit/325e7bbe7822813c889eaad60390b2e7f5416320))
* **audio:** decode background music with AudioToolbox ([cebded4](https://github.com/sebastianfelipe/codetake/commit/cebded4361650d76b540a412597c755ed2ffc7e8))
* **audio:** export recordings with background music ([7f3e615](https://github.com/sebastianfelipe/codetake/commit/7f3e6159c342819f0fec0a6a44b4e6ba1b2464a4))
* **brand:** use the CodeTake logo ([3f89dd0](https://github.com/sebastianfelipe/codetake/commit/3f89dd0348b276314eabc13a45016c5a95d7b9b0))
* CodeTake MVP — local-first screen and webcam recorder for macOS ([#1](https://github.com/sebastianfelipe/codetake/issues/1)) ([7bf3fbc](https://github.com/sebastianfelipe/codetake/commit/7bf3fbcc13687b857c813d10bb47437b602cb39f))
* **macos:** add camera and microphone capture ([ab35874](https://github.com/sebastianfelipe/codetake/commit/ab35874aa09b5b9e5e9c87b68e1abbecb2f4532a))
* **macos:** add permissions and device discovery ([9404b73](https://github.com/sebastianfelipe/codetake/commit/9404b73adf50a95764c503da9a70f32373ef4ff1))
* **macos:** add screen and system audio capture ([42521af](https://github.com/sebastianfelipe/codetake/commit/42521af170c96dd16455c7cd2afb05963bf6e44e))
* **recording:** add capture abstractions and capability detection ([e058726](https://github.com/sebastianfelipe/codetake/commit/e0587266255969ff626898d50c238b058264115a))
* **recording:** add recording configuration model ([ec93cfb](https://github.com/sebastianfelipe/codetake/commit/ec93cfb68b4c2565d3676ab214a0d7c746be5ca3))
* **recording:** add recording pipeline ([be7840f](https://github.com/sebastianfelipe/codetake/commit/be7840fead0e420944deacb4e6e4309cb243c245))
* **recording:** add recording state machine and media clock ([9a01c55](https://github.com/sebastianfelipe/codetake/commit/9a01c55cc0a82b0ab54da91c540d30fb681bc873))
* **recording:** add webcam overlay compositor ([6071f90](https://github.com/sebastianfelipe/codetake/commit/6071f901172087d590c87cadeec873cd1b58dee6))
* **recording:** composite the webcam at export ([392cfb6](https://github.com/sebastianfelipe/codetake/commit/392cfb6bd23fc88ef26ac75d947566c906c8331c))
* **recording:** encode mp4 with AVAssetWriter ([02fd098](https://github.com/sebastianfelipe/codetake/commit/02fd0987adc18e6e29d36ac4eace22a0c03a1dbb))
* **recording:** place and size the webcam freely ([e1dca2a](https://github.com/sebastianfelipe/codetake/commit/e1dca2ab2194a240e1d1e92ae393e01b2c69fecf))
* **recording:** record screen and webcam as separate raw files ([9f96c75](https://github.com/sebastianfelipe/codetake/commit/9f96c75ca796ff6831669f888f076e94b7c4c38c))
* **settings:** add presets and setup validation ([86dcef7](https://github.com/sebastianfelipe/codetake/commit/86dcef75ac44204605264784e3e8ff2d10149cd2))
* **storage:** add output directory and file naming ([638ffb8](https://github.com/sebastianfelipe/codetake/commit/638ffb864c85a6c6040848236cfb2da12a7c1bff))
* **ui:** add a mock backend for browser development ([42c8555](https://github.com/sebastianfelipe/codetake/commit/42c85554a94daf95d49a946166269883786565d7))
* **ui:** add output path, formatting and overlay helpers ([c538e0f](https://github.com/sebastianfelipe/codetake/commit/c538e0f33cb056323e03aa0ca6097210b3d7e8b7))
* **ui:** add recording configuration, preview and controls ([9a1881b](https://github.com/sebastianfelipe/codetake/commit/9a1881b867f1c82f656a37a273d58e214407abb5))
* **ui:** add recording flow state machine ([0e134b2](https://github.com/sebastianfelipe/codetake/commit/0e134b25334aa9a0c81080698b86c2379dcfa698))
* **ui:** add typed backend bindings ([8454160](https://github.com/sebastianfelipe/codetake/commit/84541600ac3dd627c6a941556bdf16928a5332b5))
* **ui:** adjust the webcam in the review before saving ([3878d3c](https://github.com/sebastianfelipe/codetake/commit/3878d3c214dd198d1e610396f758f61fae139dca))
* **ui:** control recordings from the menu bar ([e04c4f3](https://github.com/sebastianfelipe/codetake/commit/e04c4f39de465497e816f441a663eda18dc58440))
* **ui:** make the menu bar indicator compact ([482af27](https://github.com/sebastianfelipe/codetake/commit/482af2748c786d35e0c40dfbb8e9c9513f57412c))
* **ui:** review recordings and add music before exporting ([2557972](https://github.com/sebastianfelipe/codetake/commit/25579728973a68958cc5ba779b0ae2837871281d))


### Bug Fixes

* **app:** satisfy clippy on Windows and Linux ([bd86c44](https://github.com/sebastianfelipe/codetake/commit/bd86c446d78ad79795983d213a429933771f1421))
* **macos:** keep permissions across local builds ([64ab912](https://github.com/sebastianfelipe/codetake/commit/64ab91259b2f6ed084dfa639322fe086d3e9a1f4))
* **macos:** sign the app bundle ([902bb79](https://github.com/sebastianfelipe/codetake/commit/902bb7917e4588fecd4fec874562dfffeb914d74))
* **recording:** finalize recordings reliably ([4edbd94](https://github.com/sebastianfelipe/codetake/commit/4edbd94a47408524abd973036077c51f7ce57f68))
* **ui:** clear the menu bar text and show the review after recording ([0a7991f](https://github.com/sebastianfelipe/codetake/commit/0a7991f1bd42ce9afac5bc473a860d85ca7f499a))


### Documentation

* add a screenshot of the app ([5d445aa](https://github.com/sebastianfelipe/codetake/commit/5d445aa656f3a14260b681b6563a1e7444ce1ccd))
* add contributing guide and code of conduct ([bdc4a3c](https://github.com/sebastianfelipe/codetake/commit/bdc4a3cab3f5ccda198c37ea4fd1540d42d3368a))
* add project overview ([f7d8949](https://github.com/sebastianfelipe/codetake/commit/f7d89496aeabcbc75ff36c0b03bfcf59de17aa06))
* document architecture, recording and roadmap ([5b5803b](https://github.com/sebastianfelipe/codetake/commit/5b5803b520f557f84aa937662d86ba226fcf8e9b))
* document development workflow ([5188e9f](https://github.com/sebastianfelipe/codetake/commit/5188e9fde6c620cf77b24fe7d0ccb1d46686d1b0))
* document raw recordings and the review step ([8a6ae2c](https://github.com/sebastianfelipe/codetake/commit/8a6ae2cfcfb5eb9060f138253bd909fc2aa45a13))
* document reviewing and exporting with music ([15ee950](https://github.com/sebastianfelipe/codetake/commit/15ee950fe7075b81c28744d904ea712e6f8e1730))
* explain the release process ([dde1188](https://github.com/sebastianfelipe/codetake/commit/dde1188701e39976e47208ad0e9a2cd6e16e14c3))
* list third-party licenses ([315dfbd](https://github.com/sebastianfelipe/codetake/commit/315dfbd0364bb397cc63e105d671248e8deabf1d))
