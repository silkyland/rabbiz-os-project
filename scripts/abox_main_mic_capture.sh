#!/usr/bin/env bash
# Run on the A21s inside scripts/device_session.sh with audio clients idle.
set -euo pipefail

if [[ $# != 3 || ! $1 =~ ^[0-9]+$ || ! $2 =~ ^(110|112)$ || $3 != /*.wav ]]; then
    echo "Usage: $0 CARD {110|112} /absolute/path.wav" >&2
    exit 2
fi
card=$1
pcm=$2
output=$3
info="/proc/asound/card$card/pcm${pcm}c/info"
[[ $(cat "/proc/asound/card$card/id") == Exynos3830* ]]
grep -Eq "^id: calliope_$((pcm - 100))([[:space:]]|$)" "$info"

controls=(
    'INP_SEL_L'
    'INP_SEL_R'
    'ADCL Mixer MIC1L Switch'
    'ADCR Mixer MIC1R Switch'
    'AMIC Bias'
    'MIC1 MIC1 On'
    'ABOX NSRC4'
    'ABOX SIFM4'
    'ABOX VPCMIN_DAI0_A'
    'ABOX VPCMIN_DAI2_A'
)
values=(
    'AMIC_L ADC_L'
    'AMIC_L ADC_R'
    1 0 1 1
    UAIF0 WDMA WDMA4 WDMA4
)
saved=()
for name in "${controls[@]}"; do
    current=$(amixer -c "$card" cget "name=$name")
    value=$(sed -n 's/^[[:blank:]]*: values=//p' <<< "$current")
    if [[ ! $value =~ ^([0-9]+|on|off)$ ]]; then
        echo "Cannot save scalar mixer value for $name" >&2
        exit 1
    fi
    saved+=("$value")
done

applied=0
restore() {
    local status=$? index
    for ((index = applied - 1; index >= 0; index--)); do
        if ! amixer -q -c "$card" cset "name=${controls[index]}" "${saved[index]}"; then
            echo "Failed to restore ${controls[index]}" >&2
            status=1
        fi
    done
    exit "$status"
}
trap restore EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

# Open the output before touching the route, and refuse to truncate existing data.
set -C
exec 3>"$output"
for index in "${!controls[@]}"; do
    applied=$((index + 1))
    amixer -q -c "$card" cset "name=${controls[index]}" "${values[index]}"
done

# 3,840 stereo S16 frames = 15,360 bytes at 48 kHz.
timeout 12s arecord -v -D "hw:$card,$pcm" -t wav -f S16_LE -c 2 -r 48000 \
    --period-size=960 --buffer-size=3840 --duration=6 - >&3
