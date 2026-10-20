#!/usr/bin/env bash
set -e

# Copies all of the tests in this directory into sibling `ui-fail-stable` and
# `ui-fail-nightly` directories. The copies are tracked as regular files, not
# symlinks, so stale copies are removed and existing ones overwritten. The
# `.stderr` snapshots in those directories are left untouched.

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

stable="${SCRIPT_DIR}/../ui-fail-stable"
nightly="${SCRIPT_DIR}/../ui-fail-nightly"
anchor="$(basename ${SCRIPT_DIR})"

echo ":: Synchronizing..."
echo "   stable: ${stable}"
echo "   nightly: ${nightly}"
echo "   anchor: ${anchor}"

for dir in "${stable}" "${nightly}"; do
  # Remove symlinks and copies whose source no longer exists.
  find "${dir}" -type l -delete
  for file in "${dir}"/*.rs; do
    [ -e "${SCRIPT_DIR}/$(basename "${file}")" ] || rm -f "${file}"
  done

  for file in "${SCRIPT_DIR}"/*.rs; do
    cp "${file}" "${dir}/"
  done
done
