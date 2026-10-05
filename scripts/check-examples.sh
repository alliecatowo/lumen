#!/bin/sh
# Type-check every examples/*.lm.md with the release build.
failed=0
for f in examples/*.lm.md; do
  echo "Checking $f..."
  ./target/release/lumen check "$f" || failed=1
done
exit $failed
