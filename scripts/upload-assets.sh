#!/usr/bin/env bash
# Upload generated media to a non-versioned GitHub Release.
# Usage: scripts/upload-assets.sh <demo|screenshots|all> [tag]
set -euo pipefail

scripts_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$scripts_dir/.." && pwd)
demo_asset="$repo_root/docs/demo/demo.gif"
screenshot_dir="$repo_root/docs/screenshots"
target=${1:-}
tag=${2:-media-assets}

usage() {
  echo "Usage: scripts/upload-assets.sh <demo|screenshots|all> [tag]" >&2
  exit 2
}

[[ -n $target ]] || usage
if [[ $tag =~ ^v?[0-9]+\.[0-9]+\.[0-9]+ ]]; then
  echo "Refusing to upload media to version tag '$tag'" >&2
  exit 1
fi

shopt -s nullglob
screenshot_files=("$screenshot_dir"/*.png)
shopt -u nullglob

case "$target" in
  demo)
    files=("$demo_asset")
    ;;
  screenshots)
    files=("${screenshot_files[@]}")
    ;;
  all)
    files=("$demo_asset" "${screenshot_files[@]}")
    ;;
  *)
    usage
    ;;
esac

if [[ $target != demo && ${#screenshot_files[@]} -eq 0 ]]; then
  echo "No screenshots found in $screenshot_dir" >&2
  exit 1
fi

declare -A asset_names=()
for file in "${files[@]}"; do
  if [[ ! -s $file ]]; then
    echo "Missing media asset: $file" >&2
    exit 1
  fi

  name=${file##*/}
  if [[ -n ${asset_names[$name]:-} ]]; then
    echo "Duplicate release asset name: $name" >&2
    exit 1
  fi
  asset_names[$name]=1
done

command -v gh >/dev/null || { echo "Missing: gh" >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "GitHub CLI is not authenticated" >&2; exit 1; }

cd "$repo_root"
repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)

if gh release view "$tag" >/dev/null 2>&1; then
  echo "Updating release $tag..."
else
  echo "Creating release $tag..."
  gh release create "$tag" \
    --target main \
    --title "Media assets" \
    --notes "Generated demo and screenshot media for README previews." \
    --latest=false
fi

gh release upload "$tag" "${files[@]}" --clobber

release_assets=$(gh release view "$tag" --json assets --jq '.assets[].name')
for name in "${!asset_names[@]}"; do
  if ! grep -Fxq -- "$name" <<<"$release_assets"; then
    echo "$name was not found on release $tag" >&2
    exit 1
  fi
done

latest_tag=$(gh api "repos/$repo/releases/latest" --jq .tag_name)
if [[ ! $latest_tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  echo "Release $tag unexpectedly became GitHub's latest release: $latest_tag" >&2
  exit 1
fi

for name in "${!asset_names[@]}"; do
  echo "Uploaded: https://github.com/$repo/releases/download/$tag/$name"
done
echo "Latest binary release remains: $latest_tag"
