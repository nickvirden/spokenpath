/** Why an input cannot be instrumented. */
export type InstrumentHtmlErrorCode =
  /** The bytes are not valid UTF-8. */
  | 'invalid-utf-8'
  /** A byte order mark or a `meta` declaration names an encoding other than UTF-8. */
  | 'unsupported-encoding'
  /** An element already carries the source attribute, which would shadow the spliced one. */
  | 'attribute-present'
  /**
   * With `classToken`, an element's class already holds a token starting with `sp-src-`, which
   * would be read back as a source location.
   */
  | 'class-token-present'
  /** The instrumented text parses to a different tree than the input. */
  | 'dom-changed';

export class InstrumentHtmlError extends Error {
  override readonly name = 'InstrumentHtmlError';
  readonly code: InstrumentHtmlErrorCode;

  constructor(code: InstrumentHtmlErrorCode, message: string) {
    super(message);
    this.code = code;
  }
}
