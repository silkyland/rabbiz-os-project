#!/bin/sh
# Read-only checks on the A21s; run locally on device or via ssh ... 'sh -s'.
set -eu
failed=0
echo '=== Running kernel ==='
uname -a
echo '=== ALSA cards ==='
cat /proc/asound/cards
if ! grep -q 'Exynos3830' /proc/asound/cards; then
    echo 'FAIL: Exynos3830 audio card absent'
    failed=1
fi
echo '=== Exynos IOMMU config ==='
zcat /proc/config.gz | grep -E 'CONFIG_EXYNOS_IOMMU[= ]'
echo '=== Power supplies ==='
charger=0
for supply in /sys/class/power_supply/*; do
    [ -f "$supply/type" ] || continue
    cat "$supply/uevent"
    case "$(cat "$supply/type")" in
        Battery) ;;
        *) charger=1 ;;
    esac
done
if [ "$charger" = 0 ]; then
    echo 'FAIL: no non-battery power supply exposed (not proof of zero physical charge current)'
    failed=1
fi
exit "$failed"
