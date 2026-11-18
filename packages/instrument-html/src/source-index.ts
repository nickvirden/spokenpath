export interface SourcePosition {
  /** 1-based; a line ends at LF, at CRLF (one break) or at a lone CR. */
  line: number;
  /** 1-based, in Unicode scalar values from the start of the line. */
  column: number;
}

/**
 * Converts parse5's offsets, which count UTF-16 code units of the decoded text, into UTF-8 byte
 * offsets into the file and editor-style line and column. Every table is built in one pass, so
 * each lookup costs the same however far into the file it is.
 */
export class SourceIndex {
  /** The file's UTF-8 byte offset of each UTF-16 offset, including one past the end. */
  readonly #bytes: Uint32Array;
  /** The number of scalar values before each UTF-16 offset, including one past the end. */
  readonly #scalars: Uint32Array;
  /** The UTF-16 offset at which each line starts, ascending. */
  readonly #lineStarts: number[] = [0];

  /**
   * `text` is the decoded file without its byte order mark; `prefixBytes` is the length of that
   * mark in the file, so byte offsets count from the file's first byte while columns do not count
   * the mark.
   */
  constructor(text: string, prefixBytes: number) {
    this.#bytes = new Uint32Array(text.length + 1);
    this.#scalars = new Uint32Array(text.length + 1);
    let bytes = prefixBytes;
    let scalars = 0;
    let index = 0;
    while (index < text.length) {
      const unit = text.charCodeAt(index);
      this.#bytes[index] = bytes;
      this.#scalars[index] = scalars;
      const isPair =
        unit >= 0xd800 &&
        unit <= 0xdbff &&
        index + 1 < text.length &&
        isLowSurrogate(text.charCodeAt(index + 1));
      if (isPair) {
        // Both halves map to the scalar's first byte; no tag boundary falls between them.
        this.#bytes[index + 1] = bytes;
        this.#scalars[index + 1] = scalars;
        bytes += 4;
        index += 2;
      } else {
        bytes += unit < 0x80 ? 1 : unit < 0x800 ? 2 : 3;
        index += 1;
      }
      scalars += 1;
      if (unit === 0x0a || (unit === 0x0d && text.charCodeAt(index) !== 0x0a)) {
        this.#lineStarts.push(index);
      }
    }
    this.#bytes[text.length] = bytes;
    this.#scalars[text.length] = scalars;
  }

  /** The file's UTF-8 byte offset of a UTF-16 offset into the decoded text. */
  byteOffset(offset: number): number {
    return this.#at(this.#bytes, offset);
  }

  position(offset: number): SourcePosition {
    let low = 0;
    let high = this.#lineStarts.length - 1;
    while (low < high) {
      const middle = (low + high + 1) >>> 1;
      if (this.#at(this.#lineStarts, middle) <= offset) {
        low = middle;
      } else {
        high = middle - 1;
      }
    }
    const lineStart = this.#at(this.#lineStarts, low);
    return {
      line: low + 1,
      column: this.#at(this.#scalars, offset) - this.#at(this.#scalars, lineStart) + 1,
    };
  }

  #at(table: ArrayLike<number>, index: number): number {
    const value = table[index];
    if (value === undefined || !Number.isInteger(index)) {
      throw new RangeError(`offset ${index} is outside the source text`);
    }
    return value;
  }
}

function isLowSurrogate(unit: number): boolean {
  return unit >= 0xdc00 && unit <= 0xdfff;
}
