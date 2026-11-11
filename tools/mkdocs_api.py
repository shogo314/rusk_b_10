"""Render the embedded Rusk API at MkDocs build time."""
import os
import re
from pathlib import Path
import subprocess

from mkdocs.exceptions import PluginError
from mkdocs.structure.files import File


ROOT = Path(__file__).resolve().parents[1]


def searchable_reference(markdown):
    output = []
    file_open = False
    entry_open = False
    fence = None
    for line in markdown.splitlines():
        marker = re.match(r'^(`{3,}|~{3,})', line)
        if marker:
            delimiter = marker.group(1)
            if fence is None:
                fence = delimiter
            elif (delimiter[0] == fence[0] and len(delimiter) >= len(fence)
                  and not line[len(delimiter):].strip()):
                fence = None
        if fence is None and re.fullmatch(r'## `[^`]+\.rusk`', line):
            if entry_open:
                output.append('\n</section>\n')
                entry_open = False
            if file_open:
                output.append('\n</section>\n')
            else:
                output.append('''
<form class="rusk-api-search" role="search" aria-label="API検索" data-api-search hidden>
  <label for="rusk-api-query">APIを検索</label>
  <div class="rusk-api-search-controls">
    <input id="rusk-api-query" type="search" name="api" autocomplete="off"
           placeholder="名前・署名・説明で検索" aria-describedby="rusk-api-status">
    <button type="reset">クリア</button>
  </div>
  <p id="rusk-api-status" role="status" aria-live="polite"></p>
</form>
''')
            output.append('\n<section class="rusk-api-file" markdown="1">\n')
            file_open = True
        entry = re.fullmatch(r'### `([^`]+)` \{ #(api-[a-z0-9-]+) \}', line)
        if fence is None and entry:
            if entry_open:
                output.append('\n</section>\n')
            output.append('\n<section class="rusk-api-entry" markdown="1">\n')
            entry_open = True
        output.append(line)
    if entry_open:
        output.append('\n</section>\n')
    if file_open:
        output.append('\n</section>\n')
    return '\n'.join(output) + '\n'


def on_files(files, config):
    revision = os.environ.get('GITHUB_SHA', 'main')
    source_url = f"{config.repo_url.rstrip('/')}/blob/{revision}/lib"
    try:
        api = subprocess.run(
            ['cargo', 'run', '--quiet', '--locked', '--', 'doc', '--source-url', source_url],
            cwd=ROOT, check=True, stdout=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise PluginError(f"Rusk API reference generation failed: {error}") from error
    files.append(File.generated(
        config, 'library/api.md', content=searchable_reference(api.stdout.decode('utf-8')),
    ))
    return files
