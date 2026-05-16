import { visit } from 'unist-util-visit';

/**
 * Remark plugin that strips rustdoc hidden lines from `rust` code blocks.
 *
 * In rustdoc, lines beginning with `# ` (hash + space) or the bare string `#`
 * are hidden from the rendered output but still compiled/checked by the
 * doc-test runner. This plugin replicates that behaviour for Docusaurus so
 * that the rendered docs match what readers would see on docs.rs.
 *
 * Rules (matching rustdoc):
 *   - `# ` followed by anything  → hidden
 *   - `#` alone (bare hash)      → hidden empty line
 *   - `#[…]` attributes          → NOT hidden (normal Rust syntax)
 */
export default function remarkRustHiddenLines() {
  return (tree) => {
    visit(tree, 'code', (node) => {
      if (node.lang !== 'rust') return;

      const lines = node.value.split('\n');
      const filtered = lines.filter(
        (line) => !(line === '#' || line.startsWith('# '))
      );
      // Drop leading blank lines that remain after hidden-line removal.
      while (filtered.length > 0 && filtered[0].trim() === '') {
        filtered.shift();
      }
      node.value = filtered.join('\n');
    });
  };
}
