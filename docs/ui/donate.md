# `/donate` — the support page

The **`/donate`** command opens a read-only page listing the ways to support
the project — three off-chain links and the crypto address — so a user who
wants to support the development finds them where they already are, in the
terminal, one command away, and copies or opens one without leaving the
conversation. It is the eleventh composer-replacing inline picker, and
deliberately the `/hooks` browser's sibling: **no text entry**, the hardware
cursor hidden, every key owned while open, the composer back on Esc.

## The support links

Three paths that need no wallet, listed first because they are the ways a
reader without one can act:

| name     | shown address                     | target                                             |
| -------- | --------------------------------- | -------------------------------------------------- |
| `Afdian` | `afdian.com/a/xynrin`             | `https://afdian.com/a/xynrin`                      |
| `Email`  | `xynrin@icloud.com`               | `mailto:xynrin@icloud.com?subject=%5BZeroth%5D%20Sponsorship` |
| `WeChat` | `github.com/yxxbc/zeroth#sponsorship` | `https://github.com/yxxbc/zeroth#sponsorship` |

Each row's shown address is an **OSC 8 hyperlink** carrying its own target
(`docs/ui/links.md`) — the same treatment `/login`'s device page gives its
URL, so a click opens the page or the mailbox in the platform's opener. The
links take no part in the `❯` selection: the page's grammar is navigation
plus *copy*, and a link row has nothing to copy into a wallet.

The WeChat row points at the repository's `#sponsorship` section rather than
carrying the 赞赏码 itself. A WeChat Pay code is a personal, WeChat-specific
2-D code, not a QR symbol this app can re-encode — decoding the image
confirms it carries no standard payload — so it is shipped as a picture
(`assets/wechat-donate.jpg`) and read from a phone, which is also the only
device that can scan one. A terminal is not that device, and a half-block
rendering of a 1152² photo is not scannable.

The catalog is the pure `app::SUPPORT_LINKS` — one `SupportLink` per row
(`name`, `shown`, `url`). `url` is passed to the opener verbatim, so it
carries its scheme: the catalog tests pin every target to `https://` or
`mailto:` with no whitespace, every name to a bare label, and every target to
be distinct.

## The addresses

| ticker | coin    | address                                       | networks   |
| ------ | ------- | --------------------------------------------- | ---------- |
| `XKO`  | X Layer | `XKO456c0e87173789d770d6a031ba320863dce6d2cc` | X Layer    |

**Each entry names its networks, and the page shows them.** One address does
not mean one chain: an EVM address is the same twenty bytes on every chain a
wallet exposes, so a page that named only one would leave a user guessing
whether the others are safe — and the wrong guess is unrecoverable. The
entries say where their address *may* be sent; the caution says, once, that
nowhere else may be. (An earlier revision named no network at all, on the
rationale that each coin had exactly one; a multi-chain address is what
retired it.)

`XKO` is X Layer's prefix on a standard twenty-byte EVM account — three
characters in place of `0x`, then forty hex digits, case-insensitive prefix
and an EIP-55 body. It is the catalog's one address, deliberately: the
project publishes a single X Layer account. A reader who pasted the `0x` form
into an X Layer transfer would be sending to an account they did not mean,
which is why the row states its network rather than leaving it to be guessed.

The catalog is the pure `app::DONATION_ADDRESSES` — one `DonationAddress`
per row (`ticker`, `coin`, `address`, and `networks`, a `&'static [&'static
str]`), in the order the page lists them. It is a const rather than a file:
the addresses are the project's, not the user's, and a page that read them
from disk would be a page anyone with write access to the config home could
redirect. The catalog tests pin every address to ASCII with no whitespace,
every ticker to a distinct uppercase word, and every entry to at least one
bare, unpadded network label — so a typo in a future entry, or an entry the
caution could not cover, fails the build rather than the donation.

## The page

```
────────────────────────────────────────────────────────────────────────
  ♥ Support Zeroth

  Free and open source, developed in the open. If it earns a place in
  your terminal, a donation keeps the work going — thank you.

  Afdian   afdian.com/a/xynrin
  Email    xynrin@icloud.com
  WeChat   github.com/yxxbc/zeroth#sponsorship

  ❯ 1. XKO  X Layer
     ╭───────────────────────────────────────────────╮
     │  XKO456c0e87173789d770d6a031ba320863dce6d2cc  │
     ╰───────────────────────────────────────────────╯
     Network: X Layer

  Send each coin only over a network listed under its address — a
  transfer on any other network cannot be recovered.

  ↑↓ navigate  enter/c copy address  esc close
────────────────────────────────────────────────────────────────────────
```

The frame is the picker family's — the same rules, the same two-column
inset, the same dim hint row — and what sits inside it is built from parts
the chrome already has, so the page reads as *this* app's rather than a
form pasted into it:

- **The title wears the banner's gradient.** `♥ Support Zeroth` is
  washed left-to-right in the header's own accent → link gradient
  (`ui::header::gradient_spans`, the builder the mascot banner uses), bold,
  behind a heart in the palette's red — the one place that hue means
  affection rather than failure. The name is read from `APP_NAME`, the one
  place the app's name lives, and a test pins it there.
- **The blurb** under it is two dim wrapped rows: what the project is and
  what a donation does. Wrapped, never cut — the page's height is its own
  line count (`docs/ui/view-flow.md`), so a narrow terminal costs a row, not a
  word.
- **The support links lead.** `Afdian   afdian.com/a/xynrin` — the name bold
  in the muted ink, the shown address in the palette's link blue, underlined
  and carrying its target as an OSC 8 hyperlink (`docs/ui/links.md`), so a
  click opens the page or the mailbox. They come before the addresses
  because they are the ways in that need no wallet, and they take no part in
  the `❯` selection: the page's grammar is navigation plus copy, and a link
  row has nothing to copy into a wallet. An address that does not fit beside
  its name wraps there, every fragment carrying the whole target.
- **Each address is a numbered row over a rounded box.** The row is the
  `/hooks` menu's shape — the `❯` marker on the highlighted one, an absolute
  `{n}.` number, the **ticker** bold, then the coin's name dim (`Bitcoin`)
  and nothing after it — no network clause rides the row, and a test reads
  each row whole to keep it that way — and the address sits beneath it inside
  the `/login` device page's rounded box, bright and bold like the one-time
  code, because it is the one thing on the page to transcribe. The box is
  sized to the address; on a terminal too narrow to seat it the address
  **wraps inside** the box (`wrap_output`, nothing cut) rather than
  overflowing the frame, and `c` is there for exactly that case.
- **The networks are a caption under the box.** `Network: X Layer` — dim,
  inset to the box's own left wall, wrapped there (a multi-chain entry's list
  would not fit a narrow pane, and a network the reader cannot see is a
  network they cannot know is safe). It
  sits *under* the box rather than on the label row for two reasons: the
  label stays one glance wide however many chains an address answers on, and
  a caption beneath a thing reads as a note about that thing. The label
  agrees with the count — `Network: X Layer`, `Networks: …` — because a
  `Network(s):` hedge reads as generated text on the one page a reader is
  checking character by character. It stays dim on the highlighted row too:
  the accent belongs to the selection, and lighting the caption would leave
  the box's border competing with it for the eye.
- **The selection is shown by colour.** The highlighted row's marker,
  number and ticker light up in the accent, and so does its box's border;
  the other rows keep the muted ink and the dim frame border. It is the
  palette's rule — the whole selected row lights up, no second caret — and
  it is what makes the box under the `❯` read as *the* address rather than
  one of several.
- **The caution is amber.** `Send each coin only over a network listed under
  its address …` wears the palette's warning colour, the ask review's
  unanswered-question hue, on a page where everything else is dim or
  bright: a wrong-network transfer is the one irreversible mistake the page
  can lead to, so it is the one line the eye is pulled to before the
  address is used. It **points at** the captions rather than restating
  them — no coin and no network named — so a catalog entry, or a chain
  added to one, is covered without touching it.
- **One blank row between blocks, never two.** The page is built as blocks
  joined by exactly one gap (the `/login` page rule), and a test walks every
  width asserting no two blank rows ever stack.

The page is **still** — nothing on it ticks — so its scrollback flow is
signed on its rows like the `/mascot` picker's.

## A terminal too short for the whole page

The whole page — title, blurb, the three links, the address's label, box and
caption, the caution and the hint — is about twenty-six rows. Bottom-anchored
on a shorter terminal, it would keep the last rows and flow the address, the
one Enter copies, off the top. So the builder takes the rows it may have and
builds the first page that fits them (`docs/ui/view-flow.md` *A page that sheds
rows before it flows*):

1. the whole page, the support links and the address boxed;
2. that page without the support links — they are the first thing a short
   terminal can spare: they cost rows, not a box, and their text is also in
   the README;
3. that page without the title and the blurb;
4. the address **alone** — its label, box and network caption — its numbered
   label (`1. XKO  X Layer`) saying where it stands. The compact pages'
   `↑ N more above` / `↓ N more below` slots (`model_compact_list`) would sit
   straight under the top rule here and hug it blank at the first address —
   the padding row the frame never keeps (`docs/ui/view-flow.md` *The frame
   hugs its content*) — and an `↑` slot that came and went with the selection
   would move the frame, so this page has none.

The accordion rung the multi-address catalog used (the addresses the highlight
is not on collapsed to their labels) is still in the builder and still
correct; with one address it builds the same rows as the page above it, so it
costs nothing and is ready for a second entry.

The caution and the hint stay on every page: the one says which networks a
coin may go over, the other how to copy it. Only a terminal shorter than the
last page bottom-anchors it and flows its top into scrollback like every
framed view; on 12 rows the lone address fits whole, its label straight
under the top rule.

## Keys

The page owns **every** key while open (routed at the top of `App::on_key`,
before the composer's global Ctrl+C/Ctrl+O), the `/hooks` grammar:

| key                | does                                                          |
| ------------------ | ------------------------------------------------------------- |
| ↑ / ↓              | move the highlight, **wrapping** at the ends (`wrap_step`)    |
| Home / End         | jump to the first / last address                              |
| `1`–`9`            | jump to that address **and copy it** (the ask modal's rule)   |
| Enter / `c`        | copy the highlighted address to the clipboard                 |
| Esc / Ctrl+C       | close — the composer returns                                  |

Anything else is swallowed: a printable key never reaches the composer draft
underneath, and neither does a paste (nothing anyone pastes is a donation
address — the `/settings` swallow rule). Copying **keeps the page open**: the
confirming toast lands above the frame with the address still in view, and a
user who wants more than one address copies the next without reopening. The page
works **mid-turn** like every picker — it only replaces the composer, and the
streaming strip keeps its rows above it — and, like every picker, it blanks
the running cell's `(ctrl+b to run in background)` hint while open
(`App::background_hint_elapsed`), since it would swallow the Ctrl+B the hint
advertises.

No text is entered anywhere on the page, so the hardware cursor is
**hidden** (`ui::cursor_visible` — the permission prompt's rule: a kitty
cursor trail would streak across the boxes on every ↑/↓) while its *seat*
still tracks the highlighted `❯` row (`menu_marker_seat`, the `/hooks` seat),
so the cursor's return when the page closes starts somewhere sensible.

## Copying an address

Enter, `c`, or a digit returns `Action::CopyDonationAddress(address)` — the
pure core decides *what* is copied, the loop does the I/O
(`tui::donate::Session::copy_donation_address`): the `/copy` clipboard path
(`clipboard::copy_to_clipboard` — arboard, with the OSC 52 fallback for a
headless, SSH or tmux session; the native lease held for the app's lifetime,
`docs/ui/copy.md`), then a transient toast — `Copied the XKO address to
clipboard`, worded for what was actually copied, the `/login` device page's
`Copied the code to clipboard` sibling — or a red `Copy failed: {reason}`.
Never a scrollback bullet (`docs/ui/toast.md`).

## API

- `app::DonationAddress` (`ticker`, `coin`, `address`, `networks`) /
  `app::DONATION_ADDRESSES` — the address catalog.
- `app::SupportLink` (`name`, `shown`, `url`) / `app::SUPPORT_LINKS` — the
  off-chain links.
- `app::DonatePicker` — the open page's state (the highlighted row);
  `App::open_donate_picker`, `close_donate_picker`, `highlighted_donation`,
  `on_key_donate_picker`.
- `Action::OpenDonatePicker` / `CloseDonatePicker` /
  `CopyDonationAddress(DonationAddress)`.
- `ui::donate_view` — `donate_view_lines` (the page builder; its length is
  the reserved height), `donate_picker_height`, `render_donate_picker`.
- `tui::donate::Session::copy_donation_address` — the clipboard write and
  the toast.

## Tests

- `app/tests/donate.rs` — both catalogs (the one address — XKO — its exact
  string, ASCII and whitespace-free, its uppercase ticker and its `X Layer`
  network, bare and unpadded; and the three support links, each with a bare
  label and an `https://` or `mailto:` target with no whitespace, all
  distinct), the XKO address's X Layer shape (a three-character prefix over
  forty hex digits), the `/donate` command opening the page (idle and
  mid-turn) and abandoning the bands that share the composer, and the whole
  key grammar (↑/↓ staying on the only row, Home/End, Enter/`c`/digit copy,
  Esc/Ctrl+C close, owns-every-key, the Ctrl+B hint clock suppressed).
- `ui/tests/donate_view.rs` — the framed page (rules, the gradient title
  naming `APP_NAME`, the blurb, every row and box, the networks captions,
  the caution, the hint),
  each support link above the addresses as a row carrying its own target,
  the links yielding whole before any address box does, a link wrapping (and
  reading back whole) at 40 columns,
  each row read whole as the ticker and the coin with nothing after it, the
  caption sitting directly under its box with the label agreeing with the
  count, aligned to the box's wall and dim under every selection, the caption
  reading back whole at 40 columns, the selection lighting its row and box,
  the address wrapping
  inside its box at a narrow width, width safety, the height contract,
  flow eligibility, the hidden cursor seated on the `❯`, and the
  no-stacked-blanks rule.
- `scripts/smoke.sh` Phase 112 — the page end to end in a real terminal:
  open from the palette, the three support links, the XKO row read whole (no
  network after the coin) over its box with `Network: X Layer` captioned
  under it, and none of the retired addresses anywhere on the page; `c`
  copies the highlighted address (the toast, and the OSC 52 fallback landing
  in tmux's paste buffer verbatim) while the page stays open, a digit past
  the one row changes nothing, and Esc brings the composer back.
