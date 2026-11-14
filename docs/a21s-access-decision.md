# Galaxy A21s (`/dev/mali0`) Access Decision (`M0-T3`)

- **Date:** 2026-11-14
- **Device:** `SM-A217F` (`R58NB2Z4CHR`), Android 12 (SDK 31), SELinux `Enforcing`
- **Test command:** `adb -s R58NB2Z4CHR shell "run-as com.termux sh -c 'id; ls -l /dev/mali0; exec 3<>/dev/mali0 && echo MALI0_RDWR_OK || echo MALI0_RDWR_DENIED'"`
- **Result:** `MALI0_RDWR_OK` (`uid=10256(u0_a256) context=u:r:runas_app:s0:c0,c257,c512,c768`, `/dev/mali0` `crw-rw-rw-`)
- **Decision:** **`UNROOTED`** — proceed to `M1` without unlocking the bootloader or tripping Knox.
