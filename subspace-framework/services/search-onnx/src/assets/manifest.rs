//! Public upstream metadata, pinned independently of the local installation.
pub(crate) const NAME: &str = "MS MARCO MiniLM L6 v2 (local ONNX, Apache-2.0)";
pub(crate) const DIRECTORY: &str = "minilm-l6-233902d25c440f23";
pub(super) const ORIGIN: &str = "https://huggingface.co/cross-encoder/ms-marco-MiniLM-L6-v2/resolve/233902d25c440f23af6f7d6e94d2946bac0bee0a";

pub(super) struct Asset {
    pub name: &'static str,
    pub remote: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub(super) const ASSETS: &[Asset] = &[
    Asset {
        name: "model.onnx",
        remote: "onnx/model.onnx",
        bytes: 91_011_230,
        sha256: "5d3e70fd0c9ff14b9b5169a51e957b7a9c74897afd0a35ce4bd318150c1d4d4a",
    },
    Asset {
        name: "config.json",
        remote: "config.json",
        bytes: 794,
        sha256: "380e02c93f431831be65d99a4e7e5f67c133985bf2e77d9d4eba46847190bacc",
    },
    Asset {
        name: "special_tokens_map.json",
        remote: "special_tokens_map.json",
        bytes: 132,
        sha256: "3c3507f36dff57bce437223db3b3081d1e2b52ec3e56ee55438193ecb2c94dd6",
    },
    Asset {
        name: "tokenizer.json",
        remote: "tokenizer.json",
        bytes: 711_396,
        sha256: "d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66",
    },
    Asset {
        name: "tokenizer_config.json",
        remote: "tokenizer_config.json",
        bytes: 1_330,
        sha256: "a5c2e5a7b1a29a0702cd28c08a399b5ecc110c263009d17f7e3b415f25905fd8",
    },
    Asset {
        name: "README.md",
        remote: "README.md",
        bytes: 3_673,
        sha256: "7c0ec39941d0d1d766ccde671f592971d82fe45b7eea550a80d1ea864ebc3baa",
    },
];
