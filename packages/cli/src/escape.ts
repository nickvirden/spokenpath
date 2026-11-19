/**
 * Whether a terminal must not receive the code point raw. The set is the one
 * `spokenpath_announce::escape_for_terminal` escapes: control characters (C0, DEL and C1, which
 * includes the CSI introducer U+009B); the bidirectional embedding, override and isolate controls
 * and the implicit direction marks; the line and paragraph separators; the zero-width characters
 * and the invisible operators; and the Unicode tag characters. Variation selectors pass, so
 * U+FE0F still selects an emoji's presentation.
 */
function needsEscape(codePoint: number): boolean {
  return (
    codePoint <= 0x1f ||
    (codePoint >= 0x7f && codePoint <= 0x9f) ||
    (codePoint >= 0x202a && codePoint <= 0x202e) ||
    (codePoint >= 0x2066 && codePoint <= 0x2069) ||
    codePoint === 0x200e ||
    codePoint === 0x200f ||
    codePoint === 0x061c ||
    codePoint === 0x2028 ||
    codePoint === 0x2029 ||
    (codePoint >= 0x200b && codePoint <= 0x200d) ||
    (codePoint >= 0x2060 && codePoint <= 0x2064) ||
    codePoint === 0xfeff ||
    (codePoint >= 0xe0000 && codePoint <= 0xe007f)
  );
}

/**
 * `text` with each character a terminal must not receive raw written as `\u{..}`, its code point
 * in lowercase hexadecimal, the form Rust's `char::escape_unicode` writes; every other character
 * passes unchanged. A lone surrogate, which a Rust string cannot hold and a terminal cannot show,
 * is escaped the same way.
 */
export function escapeForTerminal(text: string): string {
  let escaped = '';
  for (const character of text) {
    const codePoint = character.codePointAt(0) ?? 0;
    const loneSurrogate = codePoint >= 0xd800 && codePoint <= 0xdfff;
    escaped += needsEscape(codePoint) || loneSurrogate ? `\\u{${codePoint.toString(16)}}` : character;
  }
  return escaped;
}

/**
 * `text` escaped line by line with `escapeForTerminal`, keeping each `\n` as a line break, so
 * a multi-line message stays readable. A `\r` and every other control character are still
 * escaped, so no line can rewrite another.
 */
export function escapeLines(text: string): string {
  return text.split('\n').map(escapeForTerminal).join('\n');
}
