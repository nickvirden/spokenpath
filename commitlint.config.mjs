// The commit-msg hook and the pr-title CI job both read this file, so a local
// commit and a squash-merged PR title are held to the same rules.
export default {
  extends: ['@commitlint/config-conventional'],
};
