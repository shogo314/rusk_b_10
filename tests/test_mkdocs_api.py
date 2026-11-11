"""Check the generated reference sections used by browser filtering."""
from html.parser import HTMLParser
import importlib.util
from pathlib import Path
import unittest

import markdown


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('mkdocs_api', ROOT / 'tools/mkdocs_api.py')
HOOK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HOOK)


class Sections(HTMLParser):
    def __init__(self):
        super().__init__()
        self.stack = []
        self.headings = {}
        self.entries = 0
        self.forms = 0

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'section':
            kind = attrs['class']
            if kind == 'rusk-api-entry':
                assert self.stack == ['rusk-api-file']
                self.entries += 1
            self.stack.append(kind)
        if tag in ('h2', 'h3'):
            self.headings[attrs['id']] = tuple(self.stack)
        if tag == 'form':
            self.forms += 1

    def handle_endtag(self, tag):
        if tag == 'section':
            self.stack.pop()


class ApiSearchMarkup(unittest.TestCase):
    def test_overloads_and_files_keep_their_anchors_and_descriptions(self):
        source = '''# APIリファレンス

一覧です。

## `core/int.rusk`

整数の操作。

### `Int` { #api-stdlib-int-zero }

```text
extern fn Int() -> Int
```

0を返します。

### `Int` { #api-stdlib-int-value }

```text
extern fn Int(x: Int) -> Int
```

## `iter/range.rusk`

### `Range` { #api-stdlib-range }

範囲を返します。
'''
        rendered = markdown.markdown(
            HOOK.searchable_reference(source),
            extensions=['md_in_html', 'fenced_code', 'attr_list', 'toc'],
        )
        parsed = Sections()
        parsed.feed(rendered)
        self.assertEqual(parsed.entries, 3)
        self.assertEqual(parsed.forms, 1)
        self.assertEqual(parsed.stack, [])
        for anchor in ('api-stdlib-int-zero', 'api-stdlib-int-value', 'api-stdlib-range'):
            self.assertEqual(parsed.headings[anchor], ('rusk-api-file', 'rusk-api-entry'))
        self.assertIn('整数の操作。', rendered)
        self.assertIn('0を返します。', rendered)

    def test_headings_in_code_examples_do_not_create_sections(self):
        source = '''# APIリファレンス

## `core/example.rusk`

### `example` { #api-stdlib-example }

````text
## `fake.rusk`
### `fake` { #api-stdlib-fake }
```
````

~~~text
## `also-fake.rusk`
~~~

### `next` { #api-stdlib-next }

説明。
'''
        rendered = HOOK.searchable_reference(source)
        self.assertEqual(rendered.count('class="rusk-api-entry"'), 2)
        self.assertEqual(rendered.count('class="rusk-api-file"'), 1)
        self.assertEqual(rendered.count('</section>'), 3)


if __name__ == '__main__':
    unittest.main()
