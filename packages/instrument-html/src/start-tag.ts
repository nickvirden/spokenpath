/** The HTML tokenizer states a start tag passes through between its `<` and its closing `>`. */
type TagState =
  | 'tag-name'
  | 'before-attribute-name'
  | 'attribute-name'
  | 'after-attribute-name'
  | 'before-attribute-value'
  | 'attribute-value-double-quoted'
  | 'attribute-value-single-quoted'
  | 'attribute-value-unquoted'
  | 'after-attribute-value-quoted'
  | 'self-closing-start-tag';

function isWhitespace(char: string): boolean {
  return char === '\t' || char === '\n' || char === '\f' || char === '\r' || char === ' ';
}

/**
 * The offset in `text` at which an attribute can be added to the start tag spanning
 * `[start, end)`: before the self-closing `/` when the tag ends in `/>`, otherwise before the `>`.
 *
 * The exception is a tag that closes straight after an attribute's `=` (`<a href=>`,
 * `<a b = >`): there the attribute goes right after the tag name, because text spliced after the
 * `=` would become the dangling attribute's unquoted value.
 *
 * A `/` before the `>` is self-closing only when the tokenizer reads it in an attribute-separating
 * state; in `<a href=x/>` it ends the unquoted value instead. parse5's attribute locations cannot
 * decide this, because they omit attributes dropped as duplicates (`<div a=1 a=x/>`), so the tag
 * text is run through the tokenizer's start-tag states here.
 */
export function attributeInsertionOffset(text: string, start: number, end: number): number {
  if (text[start] !== '<' || text[end - 1] !== '>') {
    throw new Error(`the start tag at offset ${start} does not run from \`<\` to \`>\``);
  }
  let state: TagState = 'tag-name';
  // Where the tag name ends: the first whitespace or `/` after it, or the closing `>`.
  let tagNameEnd = end - 1;
  // A start tag's first character after `<` is an ASCII letter of the tag name.
  for (let index = start + 2; index < end; index += 1) {
    const char = text.charAt(index);
    const closes = char === '>';
    switch (state) {
      case 'tag-name':
        if (char === '/') {
          tagNameEnd = index;
          state = 'self-closing-start-tag';
        } else if (closes) {
          return finish(index, end, index);
        } else if (isWhitespace(char)) {
          tagNameEnd = index;
          state = 'before-attribute-name';
        }
        break;
      case 'attribute-name':
        // Whitespace, `/` and `>` are reconsumed in the after-attribute-name state.
        if (char === '/') {
          state = 'self-closing-start-tag';
        } else if (closes) {
          return finish(index, end, index);
        } else if (isWhitespace(char)) {
          state = 'after-attribute-name';
        } else if (char === '=') {
          state = 'before-attribute-value';
        }
        break;
      case 'after-attribute-name':
        if (char === '/') {
          state = 'self-closing-start-tag';
        } else if (closes) {
          return finish(index, end, index);
        } else if (char === '=') {
          state = 'before-attribute-value';
        } else if (!isWhitespace(char)) {
          state = 'attribute-name';
        }
        break;
      case 'before-attribute-name':
        if (char === '/') {
          state = 'self-closing-start-tag';
        } else if (closes) {
          return finish(index, end, index);
        } else if (!isWhitespace(char)) {
          // `=` here starts an attribute name (unexpected-equals-sign-before-attribute-name).
          state = 'attribute-name';
        }
        break;
      case 'before-attribute-value':
        if (char === '"') {
          state = 'attribute-value-double-quoted';
        } else if (char === "'") {
          state = 'attribute-value-single-quoted';
        } else if (closes) {
          // missing-attribute-value: the attribute's value is empty.
          return finish(index, end, tagNameEnd);
        } else if (!isWhitespace(char)) {
          state = 'attribute-value-unquoted';
        }
        break;
      case 'attribute-value-double-quoted':
        if (char === '"') {
          state = 'after-attribute-value-quoted';
        }
        break;
      case 'attribute-value-single-quoted':
        if (char === "'") {
          state = 'after-attribute-value-quoted';
        }
        break;
      case 'attribute-value-unquoted':
        if (closes) {
          return finish(index, end, index);
        }
        if (isWhitespace(char)) {
          state = 'before-attribute-name';
        }
        break;
      case 'after-attribute-value-quoted':
        if (char === '/') {
          state = 'self-closing-start-tag';
        } else if (closes) {
          return finish(index, end, index);
        } else if (isWhitespace(char)) {
          state = 'before-attribute-name';
        } else {
          // missing-whitespace-between-attributes: the character starts the next name.
          state = 'attribute-name';
        }
        break;
      case 'self-closing-start-tag':
        if (closes) {
          return finish(index, end, index - 1);
        }
        // unexpected-solidus-in-tag: the `/` is ignored and the character reconsumed.
        state = 'before-attribute-name';
        index -= 1;
        break;
    }
  }
  throw new Error(`the start tag at offset ${start} has no closing \`>\``);
}

/** Checks that the `>` the tokenizer stopped at is the one parse5 reported as the tag's end. */
function finish(closeIndex: number, end: number, insertAt: number): number {
  if (closeIndex !== end - 1) {
    throw new Error(`the start tag ending at offset ${end} closes early, at offset ${closeIndex}`);
  }
  return insertAt;
}

/** The length of the attribute name `class`, however it is cased in the source. */
const CLASS_NAME_LENGTH = 'class'.length;

/**
 * The splice that adds `token` to the value of the `class` attribute starting at `classStart` in
 * the start tag ending at `end`, so the parsed value becomes the original value, one ASCII space
 * and the token, whatever form the attribute takes:
 *
 * - quoted (`class="a"`, `class='a'`): ` <token>` before the closing quote;
 * - unquoted (`class=a`): `&#32;<token>` after the value, because a literal space would end it;
 * - `=` with no value before the tag closes (`class=>`): `" <token>"` before the `>`;
 * - no `=` at all (`class`): `=" <token>"` after the name.
 *
 * parse5's attribute location cannot place this, because it ends before an `=` that has no value
 * after it, so the attribute is scanned here as the tokenizer reads it.
 */
export function classValueEdit(
  text: string,
  classStart: number,
  end: number,
  token: string,
): { at: number; text: string } {
  const nameEnd = classStart + CLASS_NAME_LENGTH;
  let index = nameEnd;
  const skipWhitespace = () => {
    while (index < end && isWhitespace(text.charAt(index))) {
      index += 1;
    }
  };
  skipWhitespace();
  if (text.charAt(index) !== '=') {
    return { at: nameEnd, text: `=" ${token}"` };
  }
  index += 1;
  skipWhitespace();
  const first = text.charAt(index);
  if (first === '"' || first === "'") {
    const close = text.indexOf(first, index + 1);
    if (close === -1 || close >= end) {
      throw new Error(`the class attribute at offset ${classStart} has no closing quote`);
    }
    return { at: close, text: ` ${token}` };
  }
  if (first === '>') {
    return { at: index, text: `" ${token}"` };
  }
  while (index < end - 1 && !isWhitespace(text.charAt(index))) {
    index += 1;
  }
  return { at: index, text: `&#32;${token}` };
}
