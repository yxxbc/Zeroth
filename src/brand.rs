//! Brand marks: the model and provider a value names, as a terminal glyph —
//! and, on a surface with rows to spare, as a real logo.
//!
//! A model id is the honest name of what answers, but it is a wire detail:
//! `[openrouter]claude-sonnet-4` spends fourteen columns saying what one
//! single-width mark says better. The mark is chosen from the **model**
//! first — `deepseek/deepseek-v4` is DeepSeek whether an aggregator or
//! DeepSeek's own API serves it — then from the provider id, and the caller
//! keeps its text fallback (`[provider]`) for anything neither table knows.
//!
//! Detection is lexical and pure: a model id's family is its path segments,
//! read for the longest known prefix (`claude`, `gpt-5`, `deepseek-r1`, …);
//! a provider is its `providers.toml` key, with a handful of punctuation-
//! free aliases; a link is its host. Nothing here touches the network or the
//! filesystem, so every table is unit-tested and the same row on screen is
//! the same row in a capture.
//!
//! Glyphs are **single display columns** by contract — a wide mark would
//! shift every column after it, and the footer truncates by display width;
//! `brand::tests` pins it.
//!
//! The marks come from two open catalogs: [lobe-icons] / `@yldm-tech/ai-logo`
//! (MIT, the SVG logos the vendored PNGs are rendered from) and
//! [models.dev](https://models.dev) (the model-family vocabulary). Neither is
//! fetched at runtime; see `assets/brands/` and `NOTICE`.
//!
//! [lobe-icons]: https://github.com/lobehub/lobe-icons

/// A brand the app can recognise. Deliberately a closed set: an unknown
/// provider, model family or host is `None`, never a guessed look-alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Brand {
    /// Anthropic's Claude family.
    Claude,
    /// OpenAI's GPT family (and the `codex` line).
    OpenAI,
    /// Google's Gemini and Gemma families.
    Gemini,
    /// DeepSeek.
    DeepSeek,
    /// Alibaba's Qwen.
    Qwen,
    /// Moonshot's Kimi.
    Kimi,
    /// Zhipu's GLM / ChatGLM.
    Glm,
    /// xAI's Grok.
    Grok,
    /// Mistral and its Ministral / Codestral lines.
    Mistral,
    /// Meta's Llama family.
    Llama,
    /// Ollama, the local server.
    Ollama,
    /// GitHub Copilot.
    Copilot,
    /// OpenRouter.
    OpenRouter,
    /// Venice.
    Venice,
    /// Cline.
    Cline,
    /// Google Antigravity.
    Antigravity,
    /// LM Studio.
    LmStudio,
    /// llama.cpp's server.
    LlamaCpp,
    /// GitHub itself — a link host, not a model provider.
    GitHub,
    /// A coding tool in a link or config (Cursor, Codex).
    Cursor,
    /// npm and the JavaScript package registries.
    Npm,
    /// A documentation site or a general external link.
    Docs,
}

/// Which palette role a mark wears. A tone names a *meaning* the theme
/// already has a colour for, so a new brand never means a new colour in every
/// theme table (`docs/ui/theme.md`); `ui::theme::brand_color` is the one place
/// a tone becomes a `Color` (`docs/brand-icons.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// The brand accent — near the model's own ink.
    Accent,
    /// The link blue.
    Link,
    /// The success green.
    Success,
    /// The caution amber.
    Warning,
    /// The reasoning purple.
    Purple,
    /// The plain text ink.
    Text,
    /// The dim, for a mark with no claim to recognition.
    Dim,
}

/// Every model family the table knows, longest prefix first within a brand's
/// own list. The order across brands does not matter — each brand reads its
/// own list and the *first* brand that names a family wins — but within a
/// brand a longer prefix must come before a shorter one (`gpt-oss` before
/// `gpt`), which [`tests::every_family_table_is_longest_prefix_first`] pins.
struct Family {
    /// The prefix a model id's path segment starts with, lowercase.
    prefix: &'static str,
    brand: Brand,
}

/// Families are matched against **each path segment** of a model id, so a
/// vendor prefix (`anthropic/…`, `openai/…`) matches as readily as the
/// model's own name does.
const FAMILIES: &[Family] = &[
    Family {
        prefix: "claude",
        brand: Brand::Claude,
    },
    Family {
        prefix: "anthropic",
        brand: Brand::Claude,
    },
    Family {
        prefix: "gpt-oss",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "gpt",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "chatgpt",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "codex",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "o1",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "o3",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "o4",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "davinci",
        brand: Brand::OpenAI,
    },
    Family {
        prefix: "gemini",
        brand: Brand::Gemini,
    },
    Family {
        prefix: "gemma",
        brand: Brand::Gemini,
    },
    Family {
        prefix: "deepseek",
        brand: Brand::DeepSeek,
    },
    Family {
        prefix: "qwen",
        brand: Brand::Qwen,
    },
    Family {
        prefix: "qwq",
        brand: Brand::Qwen,
    },
    Family {
        prefix: "kimi",
        brand: Brand::Kimi,
    },
    Family {
        prefix: "moonshot",
        brand: Brand::Kimi,
    },
    Family {
        prefix: "glm",
        brand: Brand::Glm,
    },
    Family {
        prefix: "chatglm",
        brand: Brand::Glm,
    },
    Family {
        prefix: "grok",
        brand: Brand::Grok,
    },
    Family {
        prefix: "mistral",
        brand: Brand::Mistral,
    },
    Family {
        prefix: "ministral",
        brand: Brand::Mistral,
    },
    Family {
        prefix: "codestral",
        brand: Brand::Mistral,
    },
    Family {
        prefix: "llama",
        brand: Brand::Llama,
    },
    Family {
        prefix: "meta-llama",
        brand: Brand::Llama,
    },
    Family {
        prefix: "llama.cpp",
        brand: Brand::LlamaCpp,
    },
];

impl Brand {
    /// Every brand, in table order — what the tests walk.
    pub const ALL: &'static [Self] = &[
        Self::Claude,
        Self::OpenAI,
        Self::Gemini,
        Self::DeepSeek,
        Self::Qwen,
        Self::Kimi,
        Self::Glm,
        Self::Grok,
        Self::Mistral,
        Self::Llama,
        Self::Ollama,
        Self::Copilot,
        Self::OpenRouter,
        Self::Venice,
        Self::Cline,
        Self::Antigravity,
        Self::LmStudio,
        Self::LlamaCpp,
        Self::GitHub,
        Self::Cursor,
        Self::Npm,
        Self::Docs,
    ];

    /// The single-width mark. Chosen to read like the brand at a glance and
    /// to exist in every monospace fallback: dingbats, geometric shapes and
    /// arrows, never an emoji (a wide glyph would shift the row).
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Claude => "✻",
            Self::OpenAI => "✳",
            Self::Gemini => "✦",
            Self::DeepSeek => "◈",
            Self::Qwen => "◉",
            Self::Kimi => "☾",
            Self::Glm => "⬢",
            Self::Grok => "✕",
            Self::Mistral => "▲",
            Self::Llama => "∞",
            Self::Ollama => "◍",
            Self::Copilot => "◐",
            Self::OpenRouter => "⇄",
            Self::Venice => "♛",
            Self::Cline => "⌁",
            Self::Antigravity => "⊛",
            Self::LmStudio => "▣",
            Self::LlamaCpp => "▤",
            Self::GitHub => "⎇",
            Self::Cursor => "▸",
            Self::Npm => "▦",
            Self::Docs => "↗",
        }
    }

    /// The palette role the mark wears — the brand's nearest existing role,
    /// not a new colour (`docs/ui/theme.md`).
    #[must_use]
    pub const fn tone(self) -> Tone {
        match self {
            Self::Claude | Self::Mistral => Tone::Warning,
            Self::OpenAI | Self::OpenRouter => Tone::Success,
            Self::Gemini | Self::DeepSeek | Self::Antigravity | Self::Docs => Tone::Link,
            Self::Qwen | Self::Venice => Tone::Purple,
            Self::Kimi | Self::Ollama | Self::Copilot | Self::Cline | Self::Grok => Tone::Accent,
            Self::Llama
            | Self::LmStudio
            | Self::LlamaCpp
            | Self::GitHub
            | Self::Cursor
            | Self::Npm => Tone::Text,
            Self::Glm => Tone::Dim,
        }
    }

    /// The vendored logo file's name (`assets/brands/<name>.png`) and its
    /// pixel size — every tile is 64×64, drawn at 64 dpi so a fit keeps its
    /// square shape. `None` where the mark is a glyph only: the roomy
    /// surfaces then fall back to the glyph, never a wrong logo.
    ///
    /// The file itself is embedded ([`logo_png`](Self::logo_png)) and
    /// materialized beside the mascot, so an installed binary can draw it too
    /// (`docs/brand-icons.md`).
    pub const LOGO_PX: (u32, u32) = (64, 64);

    /// The vendored logo file's name, for the surfaces with rows to spare —
    /// `None` where the mark is a glyph only (`llama.cpp`, the link-only
    /// brands). The bytes come from [`Self::logo_png`].
    #[must_use]
    pub const fn logo(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some("claude.png"),
            Self::OpenAI => Some("openai.png"),
            Self::Gemini => Some("gemini.png"),
            Self::DeepSeek => Some("deepseek.png"),
            Self::Qwen => Some("qwen.png"),
            Self::Kimi => Some("kimi.png"),
            Self::Glm => Some("glm.png"),
            Self::Grok => Some("grok.png"),
            Self::Mistral => Some("mistral.png"),
            Self::Llama => Some("meta.png"),
            Self::Ollama => Some("ollama.png"),
            Self::Copilot => Some("copilot.png"),
            Self::OpenRouter => Some("openrouter.png"),
            Self::Venice => Some("venice.png"),
            Self::Cline => Some("cline.png"),
            Self::Antigravity => Some("antigravity.png"),
            Self::LmStudio => Some("lmstudio.png"),
            Self::LlamaCpp => Some("llamacpp.png"),
            Self::GitHub => Some("github.png"),
            Self::Cursor | Self::Npm | Self::Docs => None,
        }
    }

    /// The compiled-in tile behind [`logo`](Self::logo) — `None` for a brand
    /// with no PNG. The bytes are `assets/brands/*.png`, produced by
    /// `scripts/brand-logos.sh` from the MIT `@yldm-tech/ai-logo` set
    /// (`NOTICE`).
    #[must_use]
    pub const fn logo_png(self) -> Option<&'static [u8]> {
        match self {
            Self::Claude => Some(include_bytes!("../assets/brands/claude.png")),
            Self::OpenAI => Some(include_bytes!("../assets/brands/openai.png")),
            Self::Gemini => Some(include_bytes!("../assets/brands/gemini.png")),
            Self::DeepSeek => Some(include_bytes!("../assets/brands/deepseek.png")),
            Self::Qwen => Some(include_bytes!("../assets/brands/qwen.png")),
            Self::Kimi => Some(include_bytes!("../assets/brands/kimi.png")),
            Self::Glm => Some(include_bytes!("../assets/brands/glm.png")),
            Self::Grok => Some(include_bytes!("../assets/brands/grok.png")),
            Self::Mistral => Some(include_bytes!("../assets/brands/mistral.png")),
            Self::Llama => Some(include_bytes!("../assets/brands/meta.png")),
            Self::Ollama => Some(include_bytes!("../assets/brands/ollama.png")),
            Self::Copilot => Some(include_bytes!("../assets/brands/copilot.png")),
            Self::OpenRouter => Some(include_bytes!("../assets/brands/openrouter.png")),
            Self::Venice => Some(include_bytes!("../assets/brands/venice.png")),
            Self::Cline => Some(include_bytes!("../assets/brands/cline.png")),
            Self::Antigravity => Some(include_bytes!("../assets/brands/antigravity.png")),
            Self::LmStudio => Some(include_bytes!("../assets/brands/lmstudio.png")),
            Self::GitHub => Some(include_bytes!("../assets/brands/github.png")),
            Self::LlamaCpp | Self::Cursor | Self::Npm | Self::Docs => None,
        }
    }

    /// The brand a model row's **provider** names for its logo — the same table
    /// as [`of_provider`](Self::of_provider), but for the tiles: an
    /// aggregator (OpenRouter) shows the aggregator's own mark, not the
    /// model family's, because the *provider* is what the logo identifies.
    #[must_use]
    pub fn logo_provider(provider: &str) -> Option<Self> {
        Self::of_provider(provider).filter(|brand| brand.logo_png().is_some())
    }

    /// The brand a provider id names — its `providers.toml` key, lowercased,
    /// or one of the aliases the same backends answer to. `None` for a
    /// user-added provider, whose id says nothing about the models it serves
    /// (the model family then decides, or the text bracket stays).
    #[must_use]
    pub fn of_provider(provider: &str) -> Option<Self> {
        let provider = provider.trim().to_ascii_lowercase();
        match provider.as_str() {
            "anthropic" | "anthropic_console" | "claude" | "claude_code" => Some(Self::Claude),
            "chatgpt_codex" | "chatgpt" | "openai" | "openai_codex" => Some(Self::OpenAI),
            "openrouter" => Some(Self::OpenRouter),
            "github_copilot" | "githubcopilot" | "copilot" => Some(Self::Copilot),
            "ollama" | "ollama_cloud" => Some(Self::Ollama),
            "lmstudio" | "lm_studio" | "lm-studio" => Some(Self::LmStudio),
            "llamacpp" | "llama_cpp" | "llama-cpp" => Some(Self::LlamaCpp),
            "venice" | "a0_venice" => Some(Self::Venice),
            "cline" | "cline_pass" => Some(Self::Cline),
            "antigravity" | "google_antigravity" => Some(Self::Antigravity),
            "opencode_go" | "opencode" | "opencode_zen" => Some(Self::DeepSeek),
            _ => None,
        }
    }

    /// The brand a model id names, read from its **longest matching path
    /// segment**. `anthropic/claude-sonnet-4` reads Claude from the vendor
    /// segment; `openrouter/deepseek/deepseek-v4` reads DeepSeek from the
    /// model's own. `None` when no family matches.
    #[must_use]
    pub fn of_model(model: &str) -> Option<Self> {
        let model = model.trim().to_ascii_lowercase();
        // A trailing `:free` / `:thinking` style tag is not part of the name.
        let model = model.split(':').next().unwrap_or(&model);
        FAMILIES
            .iter()
            .find(|family| {
                model
                    .split('/')
                    .any(|segment| segment.starts_with(family.prefix))
            })
            .map(|family| family.brand)
    }

    /// The mark a model row wears: the model's own family first — the honest
    /// answer — then the provider's, and `None` when neither is known, so
    /// the caller keeps its text fallback.
    #[must_use]
    pub fn for_model(model: &str, provider: &str) -> Option<Self> {
        Self::of_model(model).or_else(|| Self::of_provider(provider))
    }

    /// The brand a link's host names. The host is matched on its registrable
    /// suffix, so `docs.anthropic.com` and `anthropic.com` agree and a
    /// look-alike (`anthropic.com.evil.example`) does not.
    #[must_use]
    pub fn of_host(host: &str) -> Option<Self> {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host);
        const TABLE: &[(&str, Brand)] = &[
            ("anthropic.com", Brand::Claude),
            ("claude.ai", Brand::Claude),
            ("claude.com", Brand::Claude),
            ("openai.com", Brand::OpenAI),
            ("chatgpt.com", Brand::OpenAI),
            ("gemini.google.com", Brand::Gemini),
            ("google.com", Brand::Gemini),
            ("deepseek.com", Brand::DeepSeek),
            ("qwen.ai", Brand::Qwen),
            ("alibabacloud.com", Brand::Qwen),
            ("moonshot.ai", Brand::Kimi),
            ("kimi.com", Brand::Kimi),
            ("zhipuai.cn", Brand::Glm),
            ("bigmodel.cn", Brand::Glm),
            ("x.ai", Brand::Grok),
            ("mistral.ai", Brand::Mistral),
            ("llama.com", Brand::Llama),
            ("ollama.com", Brand::Ollama),
            ("github.com", Brand::GitHub),
            ("githubusercontent.com", Brand::GitHub),
            ("openrouter.ai", Brand::OpenRouter),
            ("venice.ai", Brand::Venice),
            ("cline.bot", Brand::Cline),
            ("antigravity.google", Brand::Antigravity),
            ("lmstudio.ai", Brand::LmStudio),
            ("npmjs.com", Brand::Npm),
            ("crates.io", Brand::Docs),
            ("docs.rs", Brand::Docs),
            ("models.dev", Brand::Docs),
        ];
        TABLE.iter().find_map(|(suffix, brand)| {
            (host == *suffix || host.ends_with(&format!(".{suffix}"))).then_some(*brand)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn every_glyph_is_one_column_and_carries_no_space() {
        for brand in Brand::ALL {
            let glyph = brand.glyph();
            assert!(!glyph.is_empty(), "{brand:?} has no mark");
            assert_eq!(
                UnicodeWidthStr::width(glyph),
                1,
                "{brand:?}'s mark {glyph:?} is not one display column"
            );
            assert!(!glyph.contains(char::is_whitespace), "{brand:?}");
        }
    }

    #[test]
    fn every_brand_is_distinct_and_logos_are_unique_file_names() {
        let mut glyphs = std::collections::HashSet::new();
        let mut logos = std::collections::HashSet::new();
        for brand in Brand::ALL {
            assert!(glyphs.insert(brand.glyph()), "{brand:?}'s mark repeats");
            if let Some(logo) = brand.logo() {
                assert!(logos.insert(logo), "logo {logo:?} is claimed twice");
                assert!(logo.ends_with(".png"), "{logo:?} is not a PNG name");
            }
        }
        assert_eq!(Brand::ALL.len(), glyphs.len(), "two brands share a mark");
    }

    #[test]
    fn every_family_table_is_longest_prefix_first() {
        // Within each brand's run, a shorter prefix must never precede one of
        // its own extensions — `gpt` before `gpt-oss` would claim the OSS line
        // as plain GPT (harmless today, wrong the day the brands split).
        for (index, family) in FAMILIES.iter().enumerate() {
            for later in &FAMILIES[index + 1..] {
                if later.brand != family.brand {
                    continue;
                }
                assert!(
                    !later.prefix.starts_with(family.prefix),
                    "{:?}: {:?} is read as {:?} before its own prefix",
                    family.brand,
                    later.prefix,
                    family.prefix
                );
            }
        }
    }

    #[test]
    fn a_model_family_beats_the_provider_that_serves_it() {
        // The reported shape: an aggregator's bracket in front of a model
        // whose own family is what the reader recognises.
        assert_eq!(
            Brand::for_model("deepseek/deepseek-v4.1-flash", "openrouter"),
            Some(Brand::DeepSeek)
        );
        assert_eq!(
            Brand::for_model("anthropic/claude-sonnet-4", "unknown_gateway"),
            Some(Brand::Claude)
        );
        assert_eq!(
            Brand::for_model("gpt-5.5", "chatgpt_codex"),
            Some(Brand::OpenAI)
        );
        assert_eq!(Brand::for_model("qwen3-coder-plus", ""), Some(Brand::Qwen));
        assert_eq!(
            Brand::for_model("kimi-k2", "ollama_cloud"),
            Some(Brand::Kimi)
        );
        assert_eq!(Brand::for_model("glm-5.2", "zhipu"), Some(Brand::Glm));
    }

    #[test]
    fn the_provider_is_the_fallback_and_unknown_ids_stay_unknown() {
        // A model name that says nothing — a local checkpoint — falls back to
        // the provider, and a user-added provider falls back to the caller's
        // `[provider]` text: never a guessed mark.
        assert_eq!(
            Brand::for_model("my-finetune-v3", "ollama"),
            Some(Brand::Ollama)
        );
        assert_eq!(Brand::for_model("my-finetune-v3", "my_gateway"), None);
        assert_eq!(Brand::of_model("some-random-model"), None);
        assert_eq!(Brand::of_provider("my_gateway"), None);
        assert_eq!(Brand::of_provider(""), None);
    }

    #[test]
    fn model_ids_are_read_case_insensitively_and_ignore_a_trailing_tag() {
        assert_eq!(Brand::of_model("Claude-Sonnet-4"), Some(Brand::Claude));
        assert_eq!(
            Brand::of_model("openrouter/deepseek-r1:free"),
            Some(Brand::DeepSeek)
        );
        assert_eq!(Brand::of_model("  gpt-4o  "), Some(Brand::OpenAI));
        // `o1`/`o3`/`o4` are prefix families, not substring scans: a name that
        // merely contains them is not OpenAI.
        assert_eq!(Brand::of_model("foo-o3-bar"), None);
        assert_eq!(Brand::of_model("o3-mini"), Some(Brand::OpenAI));
    }

    #[test]
    fn a_host_matches_on_its_suffix_not_a_lookalike() {
        assert_eq!(Brand::of_host("docs.anthropic.com"), Some(Brand::Claude));
        assert_eq!(Brand::of_host("www.github.com"), Some(Brand::GitHub));
        assert_eq!(Brand::of_host("GitHub.com"), Some(Brand::GitHub));
        assert_eq!(
            Brand::of_host("anthropic.com.evil.example"),
            None,
            "a suffixed look-alike must not borrow the brand"
        );
        assert_eq!(Brand::of_host("notgithub.com"), None);
        assert_eq!(Brand::of_host(""), None);
    }
}
