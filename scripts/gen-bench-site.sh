#!/usr/bin/env bash
# Generate a synthetic site for benchmarking: gen-bench-site.sh <dir> <pages> [meteorite-binary]
# Pages are spread over 20 sections, each with tags, a code block and a few paragraphs.
set -euo pipefail
dir=${1:?usage: gen-bench-site.sh <dir> <pages> [binary]}
n=${2:?usage: gen-bench-site.sh <dir> <pages> [binary]}
bin=${3:-meteorite}

rm -rf "$dir"
"$bin" new site "$dir" >/dev/null
rm -f "$dir"/content/blog/hello-world.md
sed -i 's/^paginate_by: 10/paginate_by: 25/' "$dir/config.yml"

body='Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.'
for s in $(seq 1 20); do
  mkdir -p "$dir/content/section-$s"
  printf -- '---\ntitle: Section %s\npaginate_by: 25\n---\n' "$s" > "$dir/content/section-$s/_index.md"
done
for i in $(seq 1 "$n"); do
  s=$(( i % 20 + 1 ))
  day=$(( i % 28 + 1 ))
  mon=$(( i % 12 + 1 ))
  printf -- '---\ntitle: "Post number %s"\ndate: 2025-%02d-%02d\ntags: [tag-%s, tag-%s, common]\n---\n%s\n\n## Details\n\n%s\n\n```rust\nfn post_%s() -> usize { %s }\n```\n\n%s\n' \
    "$i" "$mon" "$day" "$(( i % 50 ))" "$(( i % 7 ))" "$body" "$body" "$i" "$i" "$body" \
    > "$dir/content/section-$s/post-$i.md"
done
echo "generated $n pages in $dir"
