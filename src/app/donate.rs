//! The `/donate` page: the project's crypto donation addresses, and the
//! read-only inline page that shows them. See `docs/ui/donate.md`.
//!
//! The eleventh composer-replacing inline picker, and the `/hooks` browser's
//! sibling ([`super::hooks_menu`]): **no text entry** — navigation and copy
//! are the whole grammar — so there is no query, the hardware cursor hides
//! while its seat tracks the highlighted `❯` row, and every key is owned
//! while the page is open. The catalog is a const rather than a file: the
//! addresses are the project's, not the user's, and a page that read them
//! from disk would be a page anyone with write access to the config home
//! could redirect.

use super::*;

/// One donation address: the coin's ticker (`XKO`), its name (`X Layer`),
/// the address itself — copied verbatim, so it is bare ASCII with no
/// whitespace (the catalog tests pin that) — and the **networks** it is
/// reachable on.
///
/// The networks are a list rather than a single name because one address
/// need not mean one chain: an EVM address is the same twenty bytes on every
/// chain the wallet exposes, and a page that named only the first would leave
/// a user guessing whether the others are safe — a guess whose wrong answer is
/// unrecoverable. So the entry states where its address may be sent and the
/// page's caution says, once, that nothing else may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DonationAddress {
    /// The ticker the row leads with and the copy toast names — one bare
    /// uppercase word.
    pub ticker: &'static str,
    /// The coin's full name, dim beside the ticker.
    pub coin: &'static str,
    /// The address, exactly as it is to be pasted into a wallet.
    pub address: &'static str,
    /// The networks this address is reachable on, in the order the page
    /// lists them — each a bare label as a wallet's own network picker
    /// spells it. Never empty (the catalog tests pin that): an address
    /// with no network named is one the caution cannot cover.
    pub networks: &'static [&'static str],
}

/// The project's donation addresses, in the order the page lists them.
///
/// One today: `XKO`, X Layer's prefix on a standard twenty-byte EVM account.
/// The catalog is the page's only input, so an address is removed by deleting
/// its entry here — the layout, the digit grammar and the docs' examples are
/// all driven from this const (`docs/ui/donate.md`).
pub const DONATION_ADDRESSES: &[DonationAddress] = &[DonationAddress {
    ticker: "XKO",
    coin: "X Layer",
    address: "XKO456c0e87173789d770d6a031ba320863dce6d2cc",
    networks: &["X Layer"],
}];

/// One off-chain way to support the project: the row's label
/// (`Afdian`), the bare address the reader sees and copies, and the target
/// a click opens — an `https://` page or a `mailto:`.
///
/// These sit above the crypto addresses because they are the paths a reader
/// can take without owning a wallet. `url` is passed to the platform's
/// opener verbatim, so it carries the scheme it means: the page renders the
/// shown address as an OSC 8 hyperlink over it, and neither invents a
/// scheme the other would have to guess (`docs/ui/links.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SupportLink {
    /// The row's label — one bare word, like a ticker.
    pub name: &'static str,
    /// What a reader sees beside the label: the page's address, or the
    /// mailbox for a `mailto:`. Shown and copied verbatim.
    pub shown: &'static str,
    /// The target, scheme included.
    pub url: &'static str,
}

/// The project's off-chain support links, in the order the page lists them.
///
/// A const for the same reason the addresses are: these are the project's,
/// not the user's, and a page that read them from disk would be a page
/// anyone with write access to the config home could redirect.
pub const SUPPORT_LINKS: &[SupportLink] = &[
    SupportLink {
        name: "Afdian",
        shown: "afdian.com/a/xynrin",
        url: "https://afdian.com/a/xynrin",
    },
    SupportLink {
        name: "Email",
        shown: "xynrin@icloud.com",
        url: "mailto:xynrin@icloud.com?subject=%5BZeroth%5D%20Sponsorship",
    },
    SupportLink {
        name: "WeChat",
        shown: "github.com/yxxbc/zeroth#sponsorship",
        url: "https://github.com/yxxbc/zeroth#sponsorship",
    },
];

/// The open `/donate` page's state (`None` on [`App`] when closed): which
/// address the `❯` sits on. Nothing else — the catalog is a const, and the
/// page takes no text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DonatePicker {
    /// Index of the highlighted address within [`DONATION_ADDRESSES`].
    pub selected: usize,
}

impl App {
    /// Open the `/donate` page, the highlight on the first address. Abandons
    /// any `?` band / palette / file picker (they share the composer the page
    /// takes over) — every picker's open rule.
    pub fn open_donate_picker(&mut self) {
        self.shortcuts_open = false;
        self.command_menu = None;
        self.file_search = None;
        self.skill_picker = None;
        self.backtrack = Backtrack::default();
        self.donate_picker = Some(DonatePicker::default());
    }

    /// Dismiss the page (Esc, or Ctrl+C): the composer returns. No view
    /// change — it was never an overlay.
    pub fn close_donate_picker(&mut self) {
        self.donate_picker = None;
    }

    /// The highlighted address, if the page is open.
    #[must_use]
    pub fn highlighted_donation(&self) -> Option<DonationAddress> {
        let picker = self.donate_picker.as_ref()?;
        DONATION_ADDRESSES.get(picker.selected).copied()
    }

    /// Keys while the `/donate` page is open — the `/hooks` grammar. Owns
    /// **every** key (routed at the top of [`on_key`]): ↑/↓ move wrapping at
    /// the ends, Home/End jump, digits 1–9 jump to their row **and copy it**,
    /// Enter and `c` copy the highlighted address, Esc and Ctrl+C close.
    /// Copying keeps the page open, so the confirming toast lands with the
    /// address still in view and a second address is one key away.
    ///
    /// [`on_key`]: App::on_key
    pub(super) fn on_key_donate_picker(&mut self, key: KeyEvent) -> Action {
        // Ctrl+C closes the page (never quits — the picker family's rule).
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.close_donate_picker();
            return Action::CloseDonatePicker;
        }
        let len = DONATION_ADDRESSES.len();
        let Some(picker) = self.donate_picker.as_mut() else {
            return Action::None;
        };
        match key.code {
            KeyCode::Up => picker.selected = wrap_step(picker.selected, len, -1),
            KeyCode::Down => picker.selected = wrap_step(picker.selected, len, 1),
            KeyCode::Home => picker.selected = 0,
            KeyCode::End => picker.selected = len.saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('c' | 'C') => {
                if let Some(address) = DONATION_ADDRESSES.get(picker.selected) {
                    return Action::CopyDonationAddress(*address);
                }
            }
            // Digits jump to their absolute row and copy it (the ask modal's
            // and the `/hooks` menu's jump-activate rule). A digit past the
            // rows names nothing and is ignored.
            KeyCode::Char(c @ '1'..='9')
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let index = (c as usize) - ('1' as usize);
                if let Some(address) = DONATION_ADDRESSES.get(index) {
                    picker.selected = index;
                    return Action::CopyDonationAddress(*address);
                }
            }
            KeyCode::Esc => {
                self.close_donate_picker();
                return Action::CloseDonatePicker;
            }
            _ => {}
        }
        Action::None
    }
}
