---
name: Dannex Download Manager website
description: The Comet's packet label, printed in five spot inks on white label stock; the icon, the installer and the page are one print job.
colors:
  night: "#1e2a66"
  night-deep: "#070a1e"
  head: "#fff7e6"
  amber: "#ffb23e"
  ember: "#ff6a45"
  plum: "#b43a8c"
  stock: "#f5f6fb"
  paper: "#ffffff"
  paper-2: "#eceef7"
  ink: "#141a45"
  ink-2: "#39416e"
  ink-3: "#565d88"
  rule: "#1e2a66"
  line: "#d6d9ea"
  link: "#a8327f"
  on-night: "#fff7e6"
  on-night-2: "#c9cdee"
  on-night-3: "#a3a9d8"
  night-line: "rgba(255, 247, 230, .2)"
  night-paper-2: "#27357a"
  stock-dark: "#080b20"
  paper-dark: "#0d1230"
  paper-2-dark: "#151b44"
  ink-dark: "#f3f1ec"
  ink-2-dark: "#c3c7e2"
  ink-3-dark: "#9aa0c8"
  rule-dark: "#aab0da"
  line-dark: "#262d57"
  link-dark: "#f28ac8"
  bench-idm: "#8a90b5"
  bench-startup: "#c98a12"
  bench-transfer-dark: "#8fa2ff"
typography:
  display:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "clamp(40px, 5.4vw, 80px)"
    fontWeight: 900
    lineHeight: 0.98
    letterSpacing: "-0.03em"
    fontVariation: "'wdth' 110"
  hero-headline:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "clamp(32px, 3.05vw, 46px)"
    fontWeight: 850
    lineHeight: 1.02
    letterSpacing: "-0.022em"
    fontVariation: "'wdth' 100"
  page-title:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "clamp(36px, 5vw, 66px)"
    fontWeight: 850
    lineHeight: 1.02
    letterSpacing: "-0.025em"
    fontVariation: "'wdth' 116"
  headline:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "clamp(30px, 3.6vw, 50px)"
    fontWeight: 850
    lineHeight: 1.02
    letterSpacing: "-0.022em"
    fontVariation: "'wdth' 116"
  measure-figure:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "clamp(64px, 9vw, 96px)"
    fontWeight: 900
    lineHeight: 0.86
    letterSpacing: "-0.035em"
    fontFeature: "'tnum' 1"
    fontVariation: "'wdth' 120"
  title:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "clamp(25px, 2.5vw, 31px)"
    fontWeight: 850
    lineHeight: 1.06
    letterSpacing: "-0.018em"
    fontVariation: "'wdth' 110"
  band:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "16px"
    fontWeight: 850
    letterSpacing: "-0.005em"
    fontVariation: "'wdth' 116"
  lede:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "19px"
    fontWeight: 400
    lineHeight: 1.5
    fontVariation: "'wdth' 100"
  body:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.6
    fontVariation: "'wdth' 100"
  field-value:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "16px"
    fontWeight: 700
    lineHeight: 1.35
    fontFeature: "'tnum' 1"
  field-name:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "11.5px"
    fontWeight: 750
    lineHeight: 1.3
    letterSpacing: "0.09em"
    fontVariation: "'wdth' 108"
  button:
    fontFamily: "Archivo, Segoe UI, system-ui, -apple-system, Noto Sans, sans-serif"
    fontSize: "16px"
    fontWeight: 750
    lineHeight: 1.05
    fontVariation: "'wdth' 104"
  mono:
    fontFamily: "Cascadia Mono, Consolas, SF Mono, ui-monospace, Menlo, monospace"
    fontSize: "0.88em"
rounded:
  tag: "4px"
  row: "6px"
  button: "8px"
  label: "10px"
  night-field: "14px"
spacing:
  gutter: "clamp(16px, 4vw, 40px)"
  section: "clamp(56px, 7vw, 96px)"
  wrap: "1240px"
  wrap-narrow: "880px"
  field-row: "9px 0 10px"
components:
  button-go:
    backgroundColor: "{colors.ember}"
    textColor: "{colors.night-deep}"
    typography: "{typography.button}"
    rounded: "{rounded.button}"
    padding: "0 22px"
    height: "48px"
  button-go-hover:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.night-deep}"
  button-line:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.button}"
    padding: "0 22px"
    height: "48px"
  label-stock:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.ink}"
    rounded: "{rounded.label}"
  label-night:
    backgroundColor: "{colors.night}"
    textColor: "{colors.on-night}"
    rounded: "{rounded.night-field}"
  sheet-band:
    backgroundColor: "{colors.night}"
    textColor: "{colors.on-night}"
    typography: "{typography.band}"
    rounded: "{rounded.row}"
    padding: "12px 24px"
  tag-stable:
    backgroundColor: "{colors.rule}"
    textColor: "{colors.paper}"
    typography: "{typography.field-name}"
    rounded: "{rounded.tag}"
    padding: "3px 8px"
  tag-recommended:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.field-name}"
    rounded: "{rounded.tag}"
    padding: "3px 8px"
  segment-selected:
    backgroundColor: "{colors.rule}"
    textColor: "{colors.paper}"
    rounded: "{rounded.button}"
    padding: "10px 16px"
  stamp:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.night-deep}"
    padding: "0.7em 1.2em"
  nav-link:
    textColor: "{colors.ink}"
    rounded: "{rounded.row}"
    padding: "8px 9px"
---

# Design System: Dannex Download Manager website

## Overview

**Creative North Star: "The Comet's Packet Label"**

DDM is sold the way a sky-shot firework is: on its printed packet label. The site is that label, offset-printed in the Comet icon's own spot inks (Night, Night deep, Head, Amber, Ember, Plum) on white label stock, so the icon, the installer and the page read as one print job. The persuasive moments (the hero, the contents, the race, the delivery options, the languages, the side-by-side comparison, "built in the open", the close, the download and IDM-alternative title labels, the 404) are night fields: Night ink panels framed in a double amber rule with a four-point spark at each corner. Everything between them is stock, and the labels on stock are white, framed in a night double rule.

The voice is billing: Archivo's width axis expanded and heavy for what sells, plain for what is read, tabular figures for every measure. Facts are printed the way a packet lists its contents, a name at the start and its measure at the end across a dotted leader. Print is flat. Rules and frames separate things; nothing floats, except the real app window (and the feature page's paper tag laid on it), which is a photograph placed on the print and so casts a shadow. The comet itself, three streaks falling into a white-hot head, strikes the real window once on arrival and then rests.

Read pages (help, FAQ, news, legal) take the same world with restraint: the title on stock, one framed stock label to read, night appearing only as small marks (the current doc-nav item, step discs, the crew tag).

**Key Characteristics:**
- Five spot inks from the icon, never a sixth hue; light stock with white labels, night-deep stock with night labels in dark.
- Night fields re-map the reading inks locally, so any component printed inside one takes the on-night inks without its own dark variant.
- Double rules everywhere a frame is drawn: amber on night, night on stock.
- Archivo variable, self-hosted, expanded for billing (wdth 104-122), 100% for reading.
- Ember is the Download action and arrival; amber frames and names things on night; plum links on stock.
- Flat print, one exception: real screenshots cast a shadow.
- States differ by mark (a spark, a fill, a rule, a dash), never by colour alone.

## Colors

Five spot inks from the Comet icon, printed on a cool white stock; every reading value is checked AA on its ground.

### Primary
- **Night** (night): the label's ground ink. Fills every night field and every sheet band, rules every stock label (as rule), fills selected segments and stable tags, and the night tag over a crew photo. Identical in both themes: it is the icon's tile.
- **Night Deep** (night-deep): the darkest ink. The footer's ground, the words on Download and on the seal, the barcode's bars on its Head patch.

### Secondary
- **Ember** (ember): by law, the Download action (the split button, its arrow half, the header's Download) and arrival (the race's finish, ours ticked "arrived" in the declaration). Also carried inside the comet's streak gradient (plum to ember to amber) wherever a transfer is drawn: the race's running lane, the benchmark speed-up bars, the side-by-side time bars, the connections filling in ours-row. The ember impression behind night headlines is the one decorative use, and it is a print effect, not a fill.
- **Amber** (amber): the frame and name ink on night. The double rule round night fields, the corner sparks, field names on the hero's contents strip, footer column heads, links on night (via link), the race's big ratio, the seal's starburst (stamp), focus on night and in dark. Also the Download hover.

### Tertiary
- **Plum** (plum): the comet's tail. Focus ring on light stock, the hover on the crew pile's "+N" disc and a feature arrow's hover. Links on stock print in a plum darkened for reading (link); in dark, link-dark.
- **Head** (head): the comet's white-hot head. Night headlines are billed in it (the hero, the close, the title labels, the 404); the barcode prints on a Head patch; on-night text is Head.

### Neutral
- **Light Stock** (stock): the page itself, under the header and between labels.
- **Label White** (paper) and **Label Tint** (paper-2): the stock label's face, and its hover, selected-row and code ground.
- **Night Ink for Reading** (ink, ink-2, ink-3): body, secondary and fine print on stock; a night darkened for reading.
- **Rule** (rule): the 1.5px frame ink, Night in light, rule-dark in dark.
- **Hairline** (line): 1px separators between rows inside a label.
- **On-Night Inks** (on-night, on-night-2, on-night-3, night-line, night-paper-2): what a night field prints in, its hairline, and its raised tint for hover and selected rows.
- **Dark stock** (stock-dark, paper-dark, paper-2-dark, ink-dark, ink-2-dark, ink-3-dark, rule-dark, line-dark, link-dark): the dark theme's remap; the night fields stay as they are.
- **Bench inks** (bench-idm, bench-startup, bench-transfer-dark): chart series read by bench.js; IDM is always the neutral grey-blue, ours always ember.

### Named Rules
**The Ember By Law Rule.** Ember fills only the Download action and arrival. Ticks, tags, "coming soon" stickers, the sponsor heart and every other emphasis print in ink, amber or plum. The comet's own streak gradient may carry ember wherever a transfer is drawn, and the comparison's ours-row fill is the maintainer's recorded exception.

**The Local Remap Rule.** A night field never restyles its children. It re-declares ink, ink-2, ink-3, rule, line, link, focus, paper and paper-2 to the on-night inks (amber for rule, link and focus; night for paper), and everything inside follows. A new night surface joins that selector list; it does not get its own colours.

**The Five Inks Rule.** No hue outside the icon's five plus their reading tints. A new state is a new mark, not a new colour.

**The Night Third Rule.** Night ink holds at least a third of each Persuade page taken as a whole, its fields alternating with stock down the scroll; it is not owed on every screen (the project lead's reading, 2026-11-23). A light-theme stretch of stock between two night fields is fine; darkening every section is not the goal, the light theme stays light. Read pages owe no night beyond their small marks.

## Typography

**Display Font:** Archivo (variable, self-hosted, weights 100-900, width 62-125%), with Segoe UI, system-ui fallback
**Body Font:** Archivo at width 100%
**Label/Mono Font:** Cascadia Mono, Consolas, SF Mono (barcode numerals, code, the benchmark badges)

**Character:** One grotesque made to carry both voices by its width axis: expanded, heavy and tight for billing, the way a packet shouts its name; plain for reading.

### Hierarchy
- **Display** (900, clamp(40px, 5.4vw, 80px), 0.98): the close's headline, in Head on night with the ember impression. The 404 numeral (96px, wdth 122) and the poster step numerals on the themes page (68px, wdth 120) are the same voice.
- **Hero headline** (850, clamp(32px, 3.05vw, 46px), 1.02): the first viewport's billing, Head on night, ember impression; printed at width 100% so it holds in every language.
- **Page title** (850, clamp(36px, 5vw, 66px), 1.02, wdth via bill): inner page titles on stock, or on a night title label for download and IDM alternative.
- **Headline** (850, clamp(30px, 3.6vw, 50px), 1.02, bill): section heads, the largest thing in their section, with only their line under them.
- **Measure figure** (900, clamp(64px, 9vw, 96px), wdth 120, tabular): the race's ratio in amber. Lane times (28px/850) and benchmark values (26px/850) are its smaller steps.
- **Title** (850, clamp(25px, 2.5vw, 31px), head-w): prose h2, delivery option names (25px), theme names (22px).
- **Band** (850, 16px, bill): the night band that opens a labelled sheet, and the maker's line.
- **Body** (400, 17px, 1.6; prose 1.7; 16.5px on phones): reading text, prose capped at 64ch (Archivo's wide figures), list items 58ch.
- **Lede** (400, 19px, 1.5): the line under a section or page head, ink-2, 62-64ch.
- **Field value** (700, 16px, 1.35, tabular): the measure at the end of a leader row.
- **Field name** (750, 11.5px, 0.09em, uppercase, wdth 108): the label's small print: field names, table column heads, doc-nav groups, footer heads, carrier names, contents group names, tags (11px, 0.08em).

### Named Rules
**The Width Carries Rank Rule.** Rank is billed by width and size, not by colour or by labels stacked above headings. Billing widths are tokens: bill 116%, head-w 110%, field-w 108%; reading is 100%. German, Russian, Ukrainian, Polish, Dutch, Vietnamese and Indonesian narrow bill to 104% and head-w to 102%; phones (720px and under) narrow them to 106% and 104%.

**The Script Respect Rule.** Arabic, Persian, Hindi, Bengali, Thai, Chinese, Japanese and Korean get no tracking and no caps: field names and table heads rise to 13px instead, headings open to 1.35 (1.25 for CJK). Arabic and Persian print the night headlines in register, with no ember impression.

**The Measure Rule.** Every number that is a measure uses tabular figures.

## Layout

One centred column, 1240px wide with a fluid gutter (clamp(16px, 4vw, 40px)); reading pages narrow to 880px plus gutters, and docs take a 250px sticky contents rail beside the prose. Sections carry a single gap above them only (clamp(56px, 7vw, 96px)), so gaps never stack; the last block before the footer carries it below.

The home page alternates night fields and stock labels down the scroll. The hero is one night label the content width: maker line and billing on the start side (11fr), the real window on the end side (13fr) resting on the label's foot, the contents strip across the foot (four name-over-value facts and a barcode on a Head patch). Intros sit beside their content (5fr / 6fr) for languages, facts and "built in the open"; above it elsewhere.

Breakpoints: the header's nav folds into a menu at 1100px (or earlier, step by step, when a language's row doesn't fit); everything goes to one column at 980px (the hero stacks maker, window with comet, billing, contents); at 720px billing narrows, the comparison and declaration tables restack into per-row grids, and segmented tabs go vertical. The crew pile shows 7, 11 or 15 faces by width (560px / 1200px).

## Elevation & Depth

Flat print. Depth is drawn with rules, double rules and ink fields, never with shadows under text or panels. The only things that cast a shadow are things laid on the print: real app screenshots, the feature page's paper tag laid over its window (tilted 1.4deg), and pop-up menus and dialogs, which sit above the page.

### Shadow Vocabulary
- **Laid photograph** (`box-shadow: 0 1px 2px rgba(7, 10, 30, .16), 0 22px 44px -18px rgba(7, 10, 30, .55)`; deeper in dark): real screenshots, the tour's windows, the feature tag.
- **Window on night** (`box-shadow: 0 0 0 1px rgba(255, 247, 230, .22), 0 26px 50px -20px rgba(2, 3, 12, .8)`): the screenshot printed into a night field.
- **Menu** (`box-shadow: 0 2px 6px rgba(7, 10, 30, .12), 0 24px 48px -16px rgba(7, 10, 30, .45)`): the split button's menu, the header menu, the language sheet, the language offer.
- **Night double rule** (`box-shadow: inset 0 0 0 2px amber, inset 0 0 0 6px night, inset 0 0 0 7px amber at 70%`): not a shadow but the frame of every night field; the footer and the hero's strip use the same idea as a top-edge double rule.

### Named Rules
**The Only Photos Cast Shadows Rule.** If it is printed, it is flat. A shadow says "this is a real window laid on the label"; giving one to a card, a band or a heading is a lie about what it is.

## Shapes

Die-cut corners, never pills. Stock labels and plain frames are 10px; night fields 14px; buttons and segmented tabs 8px; rows, bands and menu items 6px; tags 4px. Frames are 1.5px rules: a stock label (sheet) takes a 1.5px night rule plus a 1px inner rule 3px inside at 55% (a ::after drawn above its content so row fills never cut it); a plain frame (FAQ, docs, chart cards, tour tabs) takes the single 1.5px rule. Night fields take the inset double amber rule and four-point sparks (14px, 7px outside each corner, a masked ::before in the rule ink). The header and footer edges are double rules too. Leaders are 1.5px dotted; "in transit" and claims-from-elsewhere are dashed (experimental tags, the coming-soon sticker, the said-so tick box, the make-your-own theme tile). The seal is a 22-point starburst.

## Components

### Buttons
Printed, firm, one hot one.
- **Shape:** die-cut (8px), 48px tall, 22px sides; words wrap to two lines rather than overflow.
- **Download (go):** ember with night-deep words (6.9:1) and a printed keyline: an inset 3px ember ring then a 1px night-deep line at 45%. Only for Download. As a split button its arrow half (48px square) opens the other systems in a stock menu.
- **Hover / Active:** Download turns amber; every button presses 1px down on active (.15s, cubic-bezier(.16, 1, .3, 1)). Focus is a 3px outline in focus (plum on light stock, amber on night and in dark), 2px offset.
- **Line / Ghost:** transparent with an inset 1.5px rule in currentColor; hover tints 10% of currentColor.
- **Text link:** link colour, underlined (offset 4px), 650 weight, an arrow glyph beside it.

### Tags and segmented tabs
- **Tags differ by mark:** stable is filled in rule with paper words; recommended is ruled (inset 1.5px); experimental is dashed (1.5px dashed outline). Field-name type, 4px corners. On ours-row, stable fills amber.
- **Segmented tabs** (race scenarios, benchmark switch, theme variant): one 1.5px rule round them, 1.5px dividers; the selected one is filled in rule ink with paper words and carries a 10px spark before its label.

### Cards / Containers
- **Night field:** night ground, 14px, double amber rule, sparks, on-night inks remapped locally. Hero, contents, race, delivery options, languages, side-by-side, built in the open, close, title labels, 404.
- **Stock label (sheet):** paper ground, 10px, 1.5px rule plus inner rule. A labelled sheet opens with a night band printed 6px inside its rule: what it is at the start, its fine print at the end. On night sheets the band is ruled amber underneath instead of filled.
- **Plain frame:** paper, 10px, one 1.5px rule, for reading and working parts.
- **Internal padding:** rows clamp(18px, 3vw, 32px) at the sides; labels clamp(24px, 4vw, 56px) for prose.

### Fields (leader rows)
The label's contents: each fact on one line, the field name at the start in ink-3, a 1.5px dotted leader, the measure end-aligned in field-value type; hairlines between rows. Exceptions: the hero's contents strip prints name over value (name in amber) in four columns divided by hairlines, and delivery options print them as three columns. A wide field (the feature tag) stacks name over value.

### Navigation
Slim header on the stock, 66px (60px on phones), sticky, a night double rule under it (1.5px, then 1px at 55% three pixels below). Links 15px/650, hover tints 9% of currentColor, the current page underlined 2px at 8px offset. Language button ruled at 50% of currentColor; Download in the header is the small go button (40px). Under 1100px the links move into a menu panel (stock label with menu shadow). Footer: night-deep with a double amber top rule; column heads in amber field-name type.

### The comet (signature)
The icon's comet as an SVG symbol (#comet, 400 box): three streaks in the plum-ember-amber gradient falling into a white-hot head with an amber halo, the head at 79.5% of its box both ways. It is placed by one size variable (--c) so the head lands exactly on a target corner (the hero window's top end corner, the close's seal, the title labels, the 404). Mirrored right to left. It falls in once (1.25s, cubic-bezier(.16, 1, .3, 1), .25s delay) and stops.

### The race (signature)
Two shots fired at the real benchmark times inside a night field. Each lane is one hairline axis: a dashed fuse while the app starts, then the comet's streak (plum to ember to amber; IDM's in ink-3) with a white-hot head ringed in amber (a ring, not a glow) where the bytes are arriving; the amber seal thunks onto the lane's finish (.32s, scale from 1.6). Big amber ratio under a 1.5px amber rule. Plays in real time when seen, replayable; reduced motion shows the finish.

### The seal
An amber 22-point starburst under night-deep expanded caps (900, 0.05em), set at a slant (-5 to -8deg; mirrored right to left). Used for arrival only: race finishes, the close's label, ours-row.

### Tick boxes and the declaration
18px boxes with 3px corners: filled with a drawn tick where a feature is listed, dashed where the claim is the product's own site's, empty where not listed. Ours prints on a night band across the table, amber rules above and below, its ticks ember (arrived).

## Do's and Don'ts

### Do:
- **Do** print every persuasive block as a night field (night, 14px, double amber rule, four sparks) and add new night surfaces to the local-remap selector rather than styling their children.
- **Do** frame stock labels in a 1.5px night rule with the 1px inner rule; use the single rule only for reading and working frames.
- **Do** set facts as leader rows: name at the start, dotted leader, measure end-aligned in tabular figures.
- **Do** bill with Archivo's width axis (bill 116%, head-w 110%, field-w 108%) and read at 100%; narrow for the long-word languages and phones as the tokens already do.
- **Do** mark states with a shape: the spark on a selected segment or tour window, fill / rule / dash on tags and tick boxes, underline on the current nav item.
- **Do** keep night headlines in Head with the ember impression (text-shadow .045em .04em at 80% ember), except in Arabic and Persian.
- **Do** keep motion to the grammar: the comet falls in once, the race plays in real time when seen, ours-row fills once; everything else rests, and reduced motion shows every finish.

### Don't:
- **Don't** use ember for anything but Download, arrival and the comet's own streak; ticks, tags, "coming soon", the sponsor heart and links never take it.
- **Don't** give a shadow to anything printed: cards, bands, headings and labels are flat; only real screenshots, the feature tag laid on its window, and pop-up menus cast one.
- **Don't** introduce a hue outside the five inks and their reading tints, or a second typeface for display.
- **Don't** track or uppercase Arabic, Persian, Indic, Thai or CJK text; raise it to 13px instead.
- **Don't** round frames into pills or soften the 1.5px rules; corners stay die-cut (4 / 6 / 8 / 10 / 14px).
- **Don't** put IDM artwork anywhere; IDM is named in text and drawn in neutral ink-3 (its lane, its bars, a lettered mark).
