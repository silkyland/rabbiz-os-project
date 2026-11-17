#!/bin/bash
cd "$(dirname "$0")"
i=0
for n in ember dusk aurora moon oled; do
  i=$((i+1)); [ -f $n.png ] && continue
  HF_HUB_OFFLINE=1 "$HOME/.local/bin/mflux-generate-z-image-turbo" --model mflux-community/z-image-turbo-mflux-q4 --base-model z-image-turbo \
    --prompt-file prompts/$n.txt --width 720 --height 1600 --steps 9 --seed $((40+i)) --compute-precision float16 --low-ram --make-conf --output "$PWD/$n.png" > $n.log 2>&1
  echo "$n exit=$?" >> done.log
done
echo ALL_DONE >> done.log
