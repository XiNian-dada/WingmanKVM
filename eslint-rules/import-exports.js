// Local ESLint rule: every import of the browser UI must resolve to a sibling
// module that really exports the imported names. The server embeds the files
// of web/js/app/ and serves them flat at /assets/js/<name>, so a specifier is
// always "./<name>.js". Uses only ESLint's own parser (espree); no resolver.
import fs from 'node:fs';
import path from 'node:path';
import * as espree from 'espree';

const SIBLING = /^\.\/[A-Za-z0-9._-]+\.js$/;
const cache = new Map();

// Names a module exports, or null when that cannot be known statically.
function exportedNames(file) {
  const stamp = fs.statSync(file).mtimeMs;
  const hit = cache.get(file);
  if (hit && hit.stamp === stamp) return hit.names;
  let names = new Set();
  try {
    const ast = espree.parse(fs.readFileSync(file, 'utf8'), { ecmaVersion: 'latest', sourceType: 'module' });
    for (const node of ast.body) {
      if (node.type === 'ExportDefaultDeclaration') names.add('default');
      else if (node.type === 'ExportAllDeclaration') names = null;
      else if (node.type === 'ExportNamedDeclaration') {
        const decl = node.declaration;
        if (decl?.id) names?.add(decl.id.name);
        for (const d of decl?.declarations || []) if (d.id.type === 'Identifier') names?.add(d.id.name);
        for (const s of node.specifiers) names?.add(s.exported.name ?? s.exported.value);
      }
      if (!names) break;
    }
  } catch {
    names = null; // the target's own lint run reports its syntax error
  }
  cache.set(file, { stamp, names });
  return names;
}

export default {
  rules: {
    'import-exports': {
      meta: {
        type: 'problem',
        docs: { description: 'Imports must name an existing sibling module and names it exports' },
        schema: [],
      },
      create(context) {
        function check(node) {
          if (!node.source) return;
          const specifier = node.source.value;
          if (!SIBLING.test(specifier)) {
            context.report({ node: node.source, message: `Import "${specifier}" must be a sibling module ("./<name>.js").` });
            return;
          }
          const target = path.join(path.dirname(context.filename), specifier);
          if (!fs.existsSync(target)) {
            context.report({ node: node.source, message: `Module "${specifier}" does not exist.` });
            return;
          }
          const names = exportedNames(target);
          if (!names) return;
          for (const s of node.specifiers || []) {
            const name =
              s.type === 'ImportDefaultSpecifier' ? 'default' : s.type === 'ImportSpecifier' ? s.imported.name ?? s.imported.value : null;
            if (s.type === 'ExportSpecifier') {
              const local = s.local.name ?? s.local.value;
              if (!names.has(local)) context.report({ node: s, message: `"${specifier}" does not export "${local}".` });
            } else if (name && !names.has(name)) {
              context.report({ node: s, message: `"${specifier}" does not export "${name}".` });
            }
          }
        }
        return {
          ImportDeclaration: check,
          ExportNamedDeclaration: check,
          ExportAllDeclaration: check,
          ImportExpression(node) {
            context.report({ node, message: 'Dynamic import() is not checked against the embedded modules; use a static import.' });
          },
        };
      },
    },
  },
};
