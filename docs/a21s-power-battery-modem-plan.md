# แผนพัฒนา Power Management, Battery Manager และ Modem (Cellular/Voice) บน Samsung Galaxy A21s (`SM-A217F` / Exynos 850)

เอกสารนี้วางแผนเป็น Phase สำหรับการทำให้ระบบจัดการพลังงาน, แบตเตอรี่/การชาร์จ, ความร้อน, การพักเครื่อง (Suspend), และโมเด็มโทรศัพท์ (SIM / Mobile Data / Voice Call / SMS) ใช้งานได้บน Mainline Linux (`7.3.0`) + Arch Linux ARM + SteamOS (`steamos-session`)

---

## สถานะปัจจุบัน (Verified Baseline)

จากการตรวจสอบเครื่องจริง (`a21s-steamos`) และ Kernel/DTS ปัจจุบัน:

| ส่วนประกอบ | สถานะปัจจุบัน | รายละเอียดที่พบ |
|---|---|---|
| **Power Supply (`/sys/class/power_supply/`)** | ว่างเปล่า | ยังไม่มีไดรเวอร์ Fuel Gauge / Charger (Steam/UPower ยังไม่เห็น % แบตและสถานะชาร์จ) |
| **Thermal (`/sys/class/thermal/`)** | ว่างเปล่า | ยังไม่มี Exynos TMU / Thermistor zone สำหรับควบคุมอุณหภูมิและ Thermal Throttling |
| **Main PMIC** | เปิดใช้บางส่วน | `samsung,s2mpu12-pmic` ถูกประกาศใน `src/linux/arch/arm64/boot/dts/exynos/exynos850-a217f.dts` ภายใต้ `&acpm_ipc` แล้ว |
| **Sub-PMIC / Charger / Fuel Gauge** | ยังไม่ได้พอร์ต | Vendor DTS (`a21s_eur_open_w00_r06.dts`) ใช้ `samsung,s2mu106mfd` (`s2mu106-charger`, `s2mu106-fuelgauge`, `s2mu106-usbpd`) + `samsung,sec-battery` |
| **CPUFreq Governor** | `performance` | `build/linux/.config` ตั้ง `CONFIG_CPU_FREQ_DEFAULT_GOV_PERFORMANCE=y` (ทำให้กินไฟตลอดเวลา ควรเปลี่ยนเป็น `schedutil`) |
| **Suspend (`PM_SLEEP`)** | เปิดใน config แต่ยังไม่จูน | มี `CONFIG_SUSPEND=y`, `CONFIG_PM_SLEEP=y` แต่ต้องทดสอบ wake source (`pmic_int`, `gpio-keys`, `wlan0` WoWLAN) |
| **Cellular Modem (CP)** | ยังไม่ได้พอร์ต | Vendor ใช้ `drivers/soc/samsung/cpif/` + `pmu-cp.c` (`samsung,exynos-cp`, `samsung,exynos-cp-mailbox`, `samsung,exynos-cp-shmem`) |

---

## Phase 1: Quick Wins — ลดการกินไฟทันที (CPU, Clock, Userspace)

เป้าหมาย: ลดความร้อนและยืดอายุแบตทันทีโดยไม่ต้องพอร์ตไดรเวอร์ใหม่

1. **เปลี่ยน CPUFreq Default Governor เป็น `schedutil`**
   - แก้ kernel config จาก `CONFIG_CPU_FREQ_DEFAULT_GOV_PERFORMANCE=y` เป็น `CONFIG_CPU_FREQ_DEFAULT_GOV_SCHEDUTIL=y`
   - ตั้ง `cpupower` / udev rule / systemd service ให้เลือก `schedutil` เมื่อใช้งานปกติ และปรับ `powersave` / `schedutil` ตามโปรไฟล์ของ SteamOS
2. **จัดการ Clock Gate (`clk_ignore_unused`)**
   - ปัจจุบัน `exynos850-a217f.dts` ใส่ `clk_ignore_unused` เพื่อกัน `clk_disable_unused()` ปิด clock ของ WLBT จน SoC reset
   - ระยะยาว: ระบุ clock gate ที่ WLBT/APM ต้องการให้ชัดใน DTS หรือ `scsc_platform_mif` แล้วเอา `clk_ignore_unused` ออก เพื่อให้ clock ที่ไม่ได้ใช้ถูกปิดประหยัดไฟได้จริง
3. **เปิด `cpuidle` / PSCI Deep Idle**
   - ตรวจสอบ `/sys/devices/system/cpu/cpu*/cpuidle/` ว่าเข้า C-states (`WFI`, `cpu-sleep`) ได้จริงเวลาเครื่องว่าง

---

## Phase 2: Battery Manager (S2MU106 Fuel Gauge + Charger + UPower)

เป้าหมาย: ให้ Linux เห็น `%` แบตเตอรี่, แรงดัน, กระแสชาร์จ, สถานะเสียบสาย, และให้ Steam Deck UI / UPower แสดงแบตเตอรี่ได้

### 2.1 ฮาร์ดแวร์ที่เกี่ยวข้อง (จาก Vendor DTS `a21s_eur_open_w00_r06.dts`)
- **PMIC/Charger MFD:** `samsung,s2mu106mfd` (I2C)
  - `drivers/battery/charger/s2mu106_charger/` — ควบคุมการชาร์จ, input current limit, charge current, OVP/UVLO
  - `drivers/battery/fuelgauge/s2mu106_fuelgauge/` — อ่าน SOC (%), Voltage (`Vcell`), Current (`I`), Temperature, ใช้ตาราง `battery,battery_data` จาก DTS
  - `s2mu106-usbpd` — ตรวจจับสาย Type-C / USB-PD / Fast Charging (AFC/PD)
  - `sec,s2mu106-haptic` — มอเตอร์สั่น (โบนัสสำหรับ Steam Input/UI feedback)

### 2.2 แนวทาง Implementation (แนะนำแบบ Clean Mainline Driver)
แทนที่จะยก `drivers/battery/common/` (`sec_battery`) ทั้งก้อนซึ่งผูกกับ Android wakelock/sysfs มากเกินไป แนะนำให้พอร์ตแบบ **Minimal Mainline `power_supply` class**:
1. **S2MU106 MFD Core (`s2mu106-core`):** อ่าน/เขียน register ผ่าน `regmap_i2c` (อ้างอิง register map จาก vendor `s2mu106`)
2. **S2MU106 Fuel Gauge (`power_supply` type `POWER_SUPPLY_TYPE_BATTERY`):**
   - Expose properties มาตรฐาน: `CAPACITY` (%), `VOLTAGE_NOW` (µV), `CURRENT_NOW` (µA), `TEMP` (0.1°C), `STATUS` (`Charging`/`Discharging`/`Full`), `HEALTH`, `PRESENT`
   - โหลดค่า calibration `battery,battery_data` จาก Device Tree ของ A21s
3. **S2MU106 Charger (`power_supply` type `POWER_SUPPLY_TYPE_USB` / `USB_PD`):**
   - รายงาน `ONLINE`, `USB_TYPE` (`SDP`/`DCP`/`CDP`/`PD`), ตั้งค่ากระแสชาร์จปลอดภัย (เช่น 500 mA สำหรับ USB PC, 1500–2000 mA สำหรับ Adapter)
   - ตรวจจับสาย USB (`usb0` RNDIS vs Adapter) ไม่ให้ตัดไฟหรือชาร์จเกินสเปคพอร์ต
4. **Userspace Integration:**
   - ติดตั้ง `upower` บน Arch Linux ARM (`pacman -S upower`)
   - ตรวจสอบ `upower -d` และยืนยันว่า Steam `-gamepadui` แสดงไอคอนแบตเตอรี่และเวลาคงเหลือที่มุมขวาบนได้

---

## Phase 3: Thermal Management & Deep Sleep (Suspend/Resume)

เป้าหมาย: ป้องกันเครื่องร้อนเกินขณะเล่นเกม และให้กดปุ่ม Power เพื่อ Sleep/Wake ได้เหมือนเครื่องเกมพกพา

1. **Thermal Zones (`/sys/class/thermal/`)**
   - เปิดใช้ Exynos TMU driver (`CONFIG_EXYNOS_THERMAL`) และเพิ่ม node `tmu` ใน `exynos850-a217f.dts` (CPU/GPU/ISP thermal sensors)
   - เพิ่ม ADC thermistor (`samsung,sec-thermistor` หรือ `generic-adc-thermal`) สำหรับวัดอุณหภูมิแบต/บอร์ด
   - กำหนด trip points (`passive` throttle CPU/GPU ที่ ~75–85°C, `critical` shutdown ที่ ~100–105°C)
2. **Display Backlight & Dimming**
   - ตรวจสอบการปรับความสว่างหน้าจอผ่าน `/sys/class/backlight/` เพื่อลดการใช้พลังงานจอ (จอเป็นตัวกินไฟอันดับต้นๆ) และเชื่อมกับ Steam UI Brightness slider
3. **Suspend-to-RAM (`echo mem > /sys/power/state`)**
   - ทดสอบ suspend/resume ทีละ subsystem:
     - `s2mpu12` PMIC (`pmic_int`) และ `gpio-keys` (ปุ่ม Power/Volume) เป็น wake source
     - Display panel (`syna,td4150-panel`) + Touchscreen (`ovt,td4150`) re-init หลังตื่น
     - `scsc_mx` / `scsc_wlan` / `scsc_bt` suspend/resume (หรือ unload/re-init อย่างสะอาดตอน sleep ถ้า firmware crash)
     - ผูกปุ่ม Power กับ `systemd-logind` (`HandlePowerKey=suspend`)

---

## Phase 4: Cellular Modem Driver (`cpif`) — Mobile Data, SMS และ Voice Call

เป้าหมาย: เปิดใช้งาน Exynos 850 Baseband Modem (`CP`) เพื่ออ่าน SIM, เล่นเน็ต 4G/LTE, รับ-ส่ง SMS และ **โทรออก/รับสายได้จริง**

> [!NOTE]
> ใน Exynos 850 โมเด็มอยู่ใน SoC เดียวกัน (Integrated AP+CP) สื่อสารผ่าน **Shared Memory (`shmem`) + Mailbox (`mcu_ipc`) + PMU-CP** ไม่ใช่ USB/PCIe modem ทั่วไป

### 4.1 สถาปัตยกรรมฝั่ง Kernel (อ้างอิงจาก `src/exynos850-vendor-kernel`)
1. **Reserved Memory & Firmware:**
   - Reserved memory สำหรับ `cp_shmem` และ CP binary region (ต้องเช็ก memory layout ไม่ให้ชนกับ `wifibt_if` ที่ `0xb0000000` และ Linux RAM)
   - ดึงไฟล์เฟิร์มแวร์โมเด็ม (`modem.bin` / `modem_debug.bin`) จากพาร์ติชัน `radio` / `modem` (`mmcblk0p*`) หรือ vendor partition ของ A21s
2. **PMU & Boot (`drivers/soc/samsung/pmu-cp.c` + `cpif`):**
   - `pmu-cp.c`: สั่งเปิดไฟ, reset, และ release CPU ของฝั่ง Modem (CP) ผ่าน Exynos PMU / SMC calls
   - `mcu_ipc.c` + `shm_ipc.c`: ช่องทางส่ง interrupt และ ring buffer ระหว่าง AP ↔ CP
   - `cpif` core (`modem_main.c`, `link_device_memory_sbd.c`, `ipc_io_device.c`, `net_io_device.c`):
     - สร้าง character devices สำหรับ IPC/AT/SIP เช่น `/dev/umts_ipc0`, `/dev/umts_rfs0`, `/dev/umts_boot0`
     - สร้าง network interface `rmnet0` (หรือ `umts_dm0` / `ccmni`) สำหรับ 4G Mobile Data
3. **Audio Routing สำหรับ Voice Call (สำคัญมากสำหรับการโทร!):**
   - การโทรต้องมีทั้ง **Control Plane** (ส่งคำสั่งโทร/วางสาย/รับสาย) และ **Voice Audio Path** (เสียงไมค์ ↔ โมเด็ม ↔ ลำโพง/หูฟัง)
   - ต้องตรวจสอบระบบเสียงบน A21s (ABOX / Audio DSP / Codec + PCM/TDM link ระหว่าง ABOX กับ CP) เพื่อ route เสียงสนทนาเข้า-ออก ลำโพงสนทนา/ไมโครโฟน หรือ Bluetooth HFP (`scsc_bt` ที่ทำเสร็จแล้ว!)

### 4.2 สถาปัตยกรรมฝั่ง Userspace (Linux Telephony Stack)
เนื่องจาก Samsung `cpif` ใช้โปรโตคอล **Samsung IPC (`libsamsung-ipc`)** เป็นหลัก (ไม่ใช่ AT/QMI/MBIM มาตรฐาน):
1. **Option A (แนะนำสำหรับ Linux Phone):** **`oFono` + `libsamsung-ipc` (หรือ `rilmodem` / `samsungipc` plugin)**
   - `oFono` จัดการ SIM, Network Registration, Voice Call, SMS, Data context (`rmnet0`)
   - UI/Dialer บน Linux: `gnome-calls` (โทร/รับสาย), `chatty` (SMS), เชื่อมกับ `NetworkManager` สำหรับ Mobile Data
2. **Option B (Lightweight / CLI & Script):** **`libsamsung-ipc` (`ipc-modem`)**
   - ใช้ `ipc-modem` ทดสอบ boot modem, ปลดล็อก PIN SIM, จับสัญญาณเครือข่าย, โทรออก/รับสายผ่าน CLI ได้ก่อนนำขึ้น UI
3. **Option C (Android RIL bridge):**
   - รัน `rild` / `libsec-ril.so` ใน container (`halium` / `lxc`) แล้วเชื่อมกับ `ofono-ril-binder-plugin` (ซับซ้อนกว่าแต่รองรับฟีเจอร์ครบเท่า Android vendor)

---

## ลำดับความสำคัญที่แนะนำ (Roadmap)

| ลำดับ | งาน | ความคุ้มค่า / ผลลัพธ์ | ความยาก |
|---|---|---|---|
| **1** | เปลี่ยน CPUFreq เป็น `schedutil` + เช็ก `cpuidle` | ลดร้อน/ลดกินไฟทันที | ต่ำ |
| **2** | พอร์ต **S2MU106 Fuel Gauge + Charger** + `upower` | เห็น % แบตใน Steam, ชาร์จปลอดภัย | กลาง |
| **3** | เปิด **Thermal Zones (`TMU`)** + **Backlight** + **Suspend** | กันเครื่องร้อน + กด Power Sleep ได้ | กลาง |
| **4** | พอร์ต **`cpif` + `pmu-cp` + `libsamsung-ipc`** (SIM/Data/SMS) | อ่านซิม ต่อเน็ต 4G รับ-ส่ง SMS ได้ | สูง |
| **5** | **Audio ABOX / Voice Routing** + `oFono` + `gnome-calls` | โทรออก-รับสายจริงผ่านลำโพง/BT | สูงมาก |
