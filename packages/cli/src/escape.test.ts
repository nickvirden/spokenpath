import { describe, expect, test } from 'vitest';
import { escapeForTerminal } from './escape.ts';

describe('escapeForTerminal', () => {
  test.each([
    ['ESC', '\u{1b}', '\\u{1b}'],
    ['NUL', '\u{0}', '\\u{0}'],
    ['BEL', '\u{7}', '\\u{7}'],
    ['a line feed', '\n', '\\u{a}'],
    ['DEL', '\u{7f}', '\\u{7f}'],
    ['CSI', '\u{9b}', '\\u{9b}'],
    ['the last C1 control', '\u{9f}', '\\u{9f}'],
    ['a bidi embedding', '\u{202a}', '\\u{202a}'],
    ['a bidi override', '\u{202e}', '\\u{202e}'],
    ['a bidi isolate', '\u{2066}', '\\u{2066}'],
    ['the pop directional isolate', '\u{2069}', '\\u{2069}'],
    ['the left-to-right mark', '\u{200e}', '\\u{200e}'],
    ['the right-to-left mark', '\u{200f}', '\\u{200f}'],
    ['the Arabic letter mark', '\u{61c}', '\\u{61c}'],
    ['the line separator', '\u{2028}', '\\u{2028}'],
    ['the paragraph separator', '\u{2029}', '\\u{2029}'],
    ['a zero-width space', '\u{200b}', '\\u{200b}'],
    ['a zero-width joiner', '\u{200d}', '\\u{200d}'],
    ['the word joiner', '\u{2060}', '\\u{2060}'],
    ['an invisible operator', '\u{2062}', '\\u{2062}'],
    ['the last invisible operator', '\u{2064}', '\\u{2064}'],
    ['the byte order mark', '\u{feff}', '\\u{feff}'],
    ['the first tag character', '\u{e0000}', '\\u{e0000}'],
    ['a tag letter', '\u{e0041}', '\\u{e0041}'],
    ['the cancel tag', '\u{e007f}', '\\u{e007f}'],
  ])('escapes %s', (_name, character, escaped) => {
    expect(escapeForTerminal(`a${character}b`)).toBe(`a${escaped}b`);
  });

  test.each([
    ['accented Latin', 'Café'],
    ['CJK', '日本語'],
    ['an emoji with its presentation selector', '\u{2764}\u{fe0f}'],
    ['the text presentation selector', '\u{263a}\u{fe0e}'],
    ['an ideographic variation selector', '\u{845b}\u{e0100}'],
    [
      'the neighbours of every escaped range',
      '\u{20}\u{7e}\u{a0}\u{61b}\u{61d}\u{200a}\u{2010}\u{2027}\u{202f}\u{2065}\u{206a}\u{fefe}\u{e0080}',
    ],
  ])('leaves %s unchanged', (_name, text) => {
    expect(escapeForTerminal(text)).toBe(text);
  });

  test('escapes an ANSI colour sequence so it cannot reach the terminal', () => {
    expect(escapeForTerminal('\u{1b}[31mred\u{1b}[0m')).toBe('\\u{1b}[31mred\\u{1b}[0m');
  });
});
