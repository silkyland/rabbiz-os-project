# Findings — Modem (`cpif`), Power/Battery, Thermal, Suspend & Peripherals (`SM-A217F` / Exynos 850)

เอกสารรวบรวม findings จากการสำรวจ vendor kernel (`src/exynos850-vendor-kernel`, branch `lineage-24.0`, Linux 4.19.325) เทียบกับ Mainline 7.3 (`src/linux`) — **ยังไม่มีการแก้ kernel**; เป็น groundwork สำหรับ M5/M6 ใน [ROADMAP.md](../ROADMAP.md) และขยาย [a21s-power-battery-modem-plan.md](a21s-power-battery-modem-plan.md)

Vendor repo: `github.com/samsungexynos850/android_kernel_samsung_exynos850` (`b8c71b7`) — checkout แบบ sparse, เพิ่ม path ได้ด้วย `git -C src/exynos850-vendor-kernel sparse-checkout` หรืออ่านไฟล์เดี่ยวผ่าน `git show origin/lineage-24.0:<path>` (blob:none จะดึงเฉพาะ blob นั้น)

---

## 1. Cellular Modem (`cpif`) — findings สำคัญ

### 1.1 โมเด็มคือ Shannon `s318ap` พูด **SIPC v5.0** — `libsamsung-ipc` ใช้ได้จริง

จาก `a21s_eur_open_w00_r06.dts` fragment `@modemif`:

| พารามิเตอร์ | ค่า | ความหมาย |
|---|---|---|
| `compatible` | `samsung,exynos-cp` | cpif platform driver (`modem_main.c:550`) |
| `mif,name` | `"s318ap"` | Shannon 318 ModAP (integrated AP+CP) |
| `mif,protocol` | `0` | `PROTOCOL_SIPC` (`modem_prj.h:431`) — **ไม่ใช่ SIT/MBIM** |
| `mif,ipc_version` | `0x32` (50) | `SIPC_VER_50` (`modem_v1.h:92`) — ตรงกับ `libsamsung-ipc` |
| `mif,link_name` | `"shmem"` | `LINK_DEVICE_SHMEM` — AP↔CP ผ่าน shared memory + mailbox |
| `mif,num_iodevs` | `0x16` (22) | สร้าง chardev/netdev: `umts_ipc0/1`, `umts_rfs0`, `umts_boot0`, `umts_ramdump0`, `umts_dm0`, `umts_cass`, `umts_csd`, `umts_router`, `umts_ciq0`, `rmnet0`–`rmnet7`, `multipdp{,_hiprio}`, `smd4`, `ipc_loopback0` |
| `interrupts` | SPI `0xbe` (190) | `RESET_REQ` — CP แจ้งรีเซ็ต |
| `ap2cp_cfg_addr` | `0x14020000` | AP→CP config mailbox address |

→ Option A ใน plan doc (oFono + libsamsung-ipc) เป็นเส้นทางที่ถูกต้อง: อ่าน/เขียน `/dev/umts_ipc0` + `/dev/umts_rfs0` ตาม ABI เดิม; `rmnetN` เป็น netdev สำหรับ 4G data (ไม่ใช่ QMI)

### 1.2 Memory map — ไม่ชน `wifibt_if`

จาก `exynos3830.dts` reserved-memory + `cp_shmem` regions:

| Region | Address | Size | หมายเหตุ |
|---|---|---|---|
| `cp_rmem` (`exynos,modem_if`) | `0xd0000000` | `0x8800000` (136 MB) | CP binary + IPC shmem |
| ├─ CP (secure) region | +`0x0` | `0x6900000` | อ่าน map จริงจาก CP TOC (`use_mem_map_on_cp=1`) |
| `wifibt_if` | `0xb0000000` | `0xc00000` | มีอยู่แล้วใน `exynos850-a217f.dts` — ไม่ชน |
| `seclog_mem` | `0xc3000000` | `0x80000` | debug-snapshot log |
| `gnss_rmem` | `0xee000000` | `0x600000` | สำหรับ GPS (ข้อ 4.3) |
| `abox-slog` / `abox_dbg` | `0xe6c00000` / `0xec000000` | — | เชื่อมกับงานเสียงที่ทำอยู่ |

- `0xd0000000` อยู่ภายใน `memory` bank `0xc0000000+0x20000000` ของ `exynos850-a217f.dts` → ต้องเพิ่ม `reserved-memory` node `cp_rmem` (`no-map`) เท่านั้น ไม่ต้องแก้ memory map
- `cp_mailbox` (`samsung,exynos-cp-mailbox`) ที่ `0x11920000`, IRQ SPI `0x33` (51)

### 1.3 Kernel **ไม่** โหลด `modem.bin` — โหลดโดย bootloader หรือ userspace `cbd` (ดูข้อ 5.4)

- `pmu-cp.c`/`modem_ctrl_s5000ap.c` ไม่มี `request_firmware` ใดๆ — `shm_ipc.c` อ่าน `struct cp_toc` ที่ base ของ CP region แล้วแกะ `cp_mem_map` จากภายใน binary ที่อยู่ใน DRAM อยู่แล้ว (`cp_shmem_check_mem_map_on_cp`, `shm_ipc.c:163`)
- PIT (`docs/a21s-pit.txt`): `RADIO` → `modem.bin`, `CP_DEBUG` → `modem_debug.bin`; NV data อยู่ใน `EFS` partition
- **ทางเลือกการโหลด:** (ก) sboot-style preload → `0xd0000000` (uniLoader ต้องทำเอง), หรือ (ข) userspace อัปโหลดผ่าน `/dev/umts_boot0` แบบ daemon `cbd` ของ Android — มี open-source reference แล้ว (ข้อ 5.4) ทำให้อาจไม่ต้องแตะ uniLoader เลย

### 1.4 CP power control เป็น SMC ล้วน — port ง่าย

- `pmu-cp.c` ทำทุกอย่างผ่าน `exynos_smc(SMC_ID, READ_CTRL/WRITE_CTRL, val, reg)` กับ `CP_CTRL_NS` (`0x30`)/`CP_CTRL_S` (`0x34`): `CP_PWRON`, `CP_RESET_SET`, `CP_START`, `CP_POWER_ON`, `CP_ACTIVE_REQ_CLR`, `CP_RESET_REQ_CLR` (`include/soc/samsung/pmu-cp.h`)
- ไม่ต้องพอร์ต pmucal สำหรับ CP boot — SMC calls ไปที่ firmware (TEEGRIS/EL3 monitor) ที่ resident อยู่แล้ว
- ต้องมี `exynos_smc()` wrapper เล็กๆ — vendor `drivers/soc/samsung/exynos-smc.c` (ห่อ `arm_smccc_smc` + debug-snapshot logging ที่ตัดทิ้งได้)
- `CP_SECURE_BOOT`: signature check ทำโดย secure world ผ่าน SMC — kernel ไม่ต้อง verify เอง

### 1.5 cpif Kconfig ที่ต้องใช้ (Exynos 850 = `SEC_MODEM_S5000AP`)

```
EXYNOS_MODEM_IF=y + SEC_MODEM_S5000AP=y
→ LINK_DEVICE_SHMEM + MCU_IPC + SHM_IPC + CP_SECURE_BOOT + CP_PMUCAL + MODEM_IF_QOS + MODEM_IF_NET_GRO
```
Objects: `modem_main modem_variation modem_io_device net_io_device bootdump_io_device ipc_io_device modem_utils modem_dump cpif_clat_info cpif_tethering_info link_device{,_memory_{snapshot,flow_control,debug,sbd,legacy}} modem_notifier mcu_ipc shm_ipc modem_ctrl_s5000ap modem_ctrl` (`cpif/Makefile`)
- `EXTRA_CFLAGS -DCONFIG_OPTION_REGION` — region code สำหรับ CLAT/tethering config (`iod,option_region = "usa_spr"` ใน a21s คือ `kor`/`eur` แบบเดียวกัน — เช็กค่าตอน port)
- ไม่ต้องพอร์ต: `S5100` (PCIe modem รุ่นใหม่), `BOOT_DEVICE_SPI`, `CP_PKTPROC` (optional perf)

### 1.6 Voice call audio path — `abox_vss` พอร์ตไว้แล้ว

- `drivers/abox/abox_vss.c` อยู่ใน tree เราแล้ว (มาพร้อม abox port): ใช้ `ABOX_CPU_GEAR_CALL_KERNEL` + notifier `abox_call_notify_event(ABOX_CALL_EVENT_{ON,OFF})` — เส้นทางเสียงโทรศัพท์ผ่าน ABOX↔CP (VSS) อยู่ในไดรเวอร์เดียวกับที่กำลัง bring-up เสียง
- เชื่อมกับ Phase 5 ของ plan doc: control plane = `umts_ipc0` (SIPC), voice path = abox VSS → speaker/headset/BT HFP

---

## 2. Battery & Charger (S2MU106) — findings สำคัญ

### 2.1 Fuel gauge register map **เหมือน S2MU005 เป๊ะ** — mainline driver ใช้ได้เกือบจะทันที

เทียบ `include/linux/power/s2mu106_fuelgauge.h` (vendor) กับ `drivers/power/supply/s2mu005-battery.c` (mainline):

| Register | S2MU005 (mainline) | S2MU106 (vendor) |
|---|---|---|
| STATUS / IRQ / RVBAT / RCUR_CC / RSOC | `0x00/0x02/0x04/0x06/0x08` | เหมือนกัน |
| MONOUT / MONOUT_SEL / RBATCAP | `0x0A/0x0C/0x0E` | เหมือนกัน |
| RZADJ / RBATZ0 / RBATZ1 / IRQ_LVL / START | `0x12/0x16/0x18/0x1A/0x1E` | เหมือนกัน |

→ Minimal path: เพิ่ม `samsung,s2mu106-fuelgauge` compatible เข้า `s2mu005-battery.c` (หรือ clone) → ได้ `CAPACITY`, `VOLTAGE_{NOW,AVG}`, `CURRENT_{NOW,AVG}`, `STATUS` ทันที โดยไม่ต้องยก `sec_battery` 2,794 บรรทัด
- ความแม่นยำ: FG ต้องได้ `battery,battery_data` table (blob ยาวใน vendor dts — capacity_full `0x1388` = 5000 mAh, SDI) เขียนลงชิปตอน init; ถ้าไม่เขียน FG ใช้ profile เดิมใน NVM — % อาจเพี้ยนเล็กน้อย เทียบกับ `fuelgauge,capacity_calculation_type = <0x1c>` และ `cv_data`
- I2C: `s2mu106-fuelgauge@3B` อยู่บน **`i2c_0` (`0x13830000`)** — ทั้งสามอุปกรณ์ S2MU106 อยู่บัสเดียวกัน: MFD `s2mu106@3D` (MUIC+PM), `usbpd-s2mu106@3C`, FG `@3B`; vendor alert pin `fuelgauge,fuel_int` = GPIO + `wakeup` capable
- `i2c_0` ใน mainline `exynos850.dtsi` ตอนนี้ `status = "disabled"` ทุกตัว — เปิด `&i2c_0` + เพิ่ม node ใน a217f dts
- Driver มีอยู่แล้วใน mainline tree แต่ `CONFIG_BATTERY_S2MU005` ยังไม่ได้ set — เปิดพร้อมกัน

### 2.2 Charger / Cable detect — แยกเป็น 3 ชิ้น

1. **Charge control:** vendor `drivers/battery/charger/s2mu106_charger/` (+`s2mu106_pmeter`) — node `samsung,s2mu106-charger` ใน a21s dts ถูก `status = "disable"` (sec_battery ขับตรง); minimal path = hardcode current limit ปลอดภัยผ่าน PM regs ก่อน
2. **Cable-current table** (vendor `cable-info`): default input `0x708`=1800 mA / charging `0x834`=2100 mA; SDP=500/500, TA=1650/2400 (`0x672`/`0x960`) ฯลฯ — copy ตารางนี้มาใช้ได้
3. **MUIC (cable type detect)** อยู่ใน MFD `s2mu106@3D` — vendor framework `drivers/muic/{common,slsi/s2mu106}` ใหญ่; mainline ไม่มี; ทางเลือก: อ่าน attach type จาก MUIC regs ตรงๆ หรือ skip (ใช้ default current) — MUIC เดียวกันนี้คุม **USB role** ด้วย (OTG host vs gadget `usb0`) จึงเป็น blocker ของงาน OTG ในอนาคต
4. **USB-PD** `usbpd-s2mu106@3C` — PD contract สูงสุด 12 W (`0x1770` mV?/`0x7d0` mA → 6 V/2 A จริงๆ คือ pdic_max_voltage 6000 mV × 2000 mA); A21s ไม่มี PD sink จริง — ข้ามได้ใน phase แรก
5. **Haptic** `sec,s2mu106-haptic` (COINDC, 6 intensity steps) อยู่ใต้ MFD เดียวกัน — bonus สำหรับ Steam input feedback

### 2.3 S2MPU12 — mainline ครบอยู่แล้ว

- `sec-acpm.c` + `sec-common.c` (`s2mpu12_devs`: regulator/power-keys/rtc) + `s2mpu12-pwrkey` — ทั้งหมด upstream; `CONFIG_MFD_SEC_{CORE,ACPM,I2C}=y`, `CONFIG_INPUT_S2MPU12_PWRKEY=y` เปิดอยู่
- Node `pmic-1` ใน a217f dts มี `wakeup-source`, `system-power-controller`, WTSR (`0x3` timer)/SMPL + regulators — power key + PMIC IRQ wake พร้อม
- Vendor equivalence: `drivers/mfd/s2mpu12-{core,irq}.c` (4.19) — ไม่ต้อง port

---

## 3. Thermal, Backlight, CPUFreq/Idle, Suspend

### 3.1 Thermal — ACPM path พร้อมเกือบทั้งหมด เหลือแค่ data table

- Exynos 850 TMU ถูกคุมโดย **ACPM firmware** ผ่าน IPC channel **`0x09`** (`acpm_tmu` node ใน `exynos3830.dts`; vendor `exynos_acpm_tmu.c` → `acpm_ipc_send_data`)
- Mainline stack ครบ: `drivers/firmware/samsung/exynos-acpm.c` รองรับ `samsung,exynos850-acpm-ipc` อยู่แล้ว + ops `.tmu` generic (`acpm_tmu_init/read_temp/set_threshold/...`) + `drivers/thermal/samsung/acpm-tmu.c` (Kconfig `EXYNOS_ACPM_THERMAL=m` ตั้งไว้แล้ว)
- **Gap เดียว:** `acpm-tmu.c` bind เฉพาะ `google,gs101-tmu-top` — ต้องเพิ่ม `acpm_tmu_exynos850` driver_data (sensor group table: vendor มี zone `LITTLE0`,`LITTLE1`,`G3D`,+อื่นรวม ~9 zone ที่ `0x10070000`, IRQ SPI `0xe2`) + `samsung,exynos850-tmu` compatible + DT nodes + trip points/cooling maps
- Vendor `exynos_tmu_v2.c` (2,181 บรรทัด) เป็น fallback ถ้า ACPM path มีปัญหา — register-level TMU control; trims อยู่ใน dtsi (`tmu_efuse_value=0x37`, trims `0x19/0x55`, offset `0x32`, default trip `0x73`°C)
- หมายเหตุ: `acpm-tmu.c` ใหม่ใน mainline นี้เขียนโดย Linaro (2026) — คุ้มเช็ก lore/git ว่ามี exynos850 series ค้างอยู่ไหมก่อนเขียนเอง

### 3.2 Backlight — LM36274 มี driver ครบใน mainline

- จอ A21s ใช้ **TI LM36274** (bias + backlight) ที่ `i2c_1` addr `0x11` (vendor `lm36274@11`, `panel_i2c`)
- Mainline มี: `drivers/mfd/ti-lmu.c` (`ti,lm36274-backlight` cell) + `drivers/leds/leds-lm36274.c` + `drivers/regulator/lm363x-regulator.c` (bias `vsn/vsp`)
- TODO ใน `exynos850-a217f.dts:402` รออยู่แล้ว — เพิ่ม `&i2c_1` node + `ti,lm36274-backlight` + supplies + `backlight-gpios <&gpg2 1>` → Steam brightness slider ใช้ได้ผ่าน backlight/LED class

### 3.3 CPUFreq / CPUIdle

- OPP tables (`cpucl0`/`cpucl1`) + `acpm_ipc` clocks อยู่ใน mainline dtsi แล้ว + `CONFIG_CPUFREQ_DT=y` → CPUFreq ทำงานผ่าน `cpufreq-dt` + `clk-acpm` อยู่แล้ว; เปลี่ยนแค่ `CONFIG_CPU_FREQ_DEFAULT_GOV_PERFORMANCE=y` → `SCHEDUTIL` (`CONFIG_CPU_FREQ_GOV_SCHEDUTIL=y` build-in อยู่แล้ว)
- **cpuidle gap:** cpu nodes ไม่มี `cpu-idle-states` / `idle-states` node → ตอนนี้ idle ลึกสุดคือ default WFI; ต้องเพิ่ม `idle-states { cpu-sleep { compatible="arm,idle-state"; ... } }` + `cpu-idle-states=<&CPU_SLEEP>` ต่อ core (PSCI `enable-method="psci"` ใช้อยู่แล้ว — CPU_SUSPEND ขึ้นกับ secure firmware ที่ resident)

### 3.4 Suspend (`echo mem > /sys/power/state`)

- Wake-source infrastructure: `samsung,exynos850-wakeup-eint` controllers มีใน dtsi (`0x683`), PMIC `wakeup-source` ตั้งแล้ว — เหลือเพิ่ม `wakeup-source` ให้ `gpio-keys` (vol) ถ้าต้องการ
- Suspend path = PSCI `CPU_SUSPEND` → secure firmware; vendor `exynos-pm.c`/`pm_domains.c` ไม่ต้องพอร์ตถ้า PSCI ทำงาน
- Subsystem ที่ต้องเทสต์ resume: `decon`/`dsi` (framebuffer splash), `scsc_mx/wlan/bt`, `abox` (DSP firmware reload), `s2mu106` IRQ wake (cable insert), usb0 gadget
- Vendor suspend evidence: `wakeup-source` list ใน `exynos3830.dts:5594` มี `MAILBOX_CP2AP`, `RTC_ALARM`, `EINT`, `CP_RESET_REQ`, `WLBT_*` — แปลว่า modem ปลุกได้ผ่าน mailbox IRQ (สำหรับ incoming call/SMS ตอน sleep ในอนาคต)

---

## 4. Peripherals อื่นที่เจอระหว่างสำรวจ ("&& more")

| อุปกรณ์ | Vendor driver | Mainline ใกล้เคียง | ความเห็น |
|---|---|---|---|
| **GNSS/GPS** | `drivers/soc/samsung/gnssif/` (gnss shmem IPC, เหมือน cpif) + `gnss_rmem 0xee000000` + `gnss_mailbox` + `pmucal_gnss_exynos3830.h` | `drivers/gnss/` core framework มี upstream | GPS → `/dev/gnss0` เป็นไปได้; โหลด fw เหมือน modem (bootloader preload) |
| **NFC** | `drivers/nfc/sec_nfc.c`, node `sec-nfc@27` บน `i2c_2` | ไม่มี | ลำดับความสำคัญต่ำสำหรับ gaming |
| **Fingerprint** | `drivers/fingerprint/et5xx-spi.c` (ET520, SPI0) | ไม่มี (Android HAL protocol) | ข้าม — ไม่มี userspace บน SteamOS |
| **Sensor hub (SSP)** | `drivers/sensorhub/` (accel icm42605m/lsm6dsl + gyro) + fw `shub_nacho_a21s.bin` | ไม่มี framework ตรง | ใช้สำหรับ auto-rotate/accelerometer ในอนาคต |
| **USB OTG host** | dwc3 `dr_mode="otg"` ใน vendor dts | dwc3 ใน mainline | ตอนนี้ `usb0` เป็น gadget — role switch ผูกกับ S2MU106 MUIC (ข้อ 2.2.3) |
| **Camera** | ion heaps `camera_heap`/`secure_camera_heap` | — | out of scope |

---

## 5. งานที่มีคนทำไว้แล้ว / เอามาใช้ได้เลย (web research 2026-11-15)

### 5.1 postmarketOS มี port ของ A21s อยู่แล้ว — ใช้ tree เดียวกับเรา

- `device-samsung-a21s` + `linux-postmarketos-exynos850` (`github.com/samsungexynos850/linux` @ `8be82eac`, 7.2.0) — maintainer **Thomas Turner <tturner@lineageos.org>**; วิกิ `wiki.postmarketos.org/wiki/Samsung_Galaxy_A21s_(samsung-a21s)` (Nura), category "testing", เทสกับ Phosh
- pmOS ใช้ sboot stock + fastboot (`dtbo`/`vbmeta` partition) — ไม่ใช้ uniLoader; DTB ใน tree นั้นแยกตาม RAM: `exynos850-a217f-{3gb,4gb,6gb}.dts` (commit `8be82eac` ใหม่กว่า pin `14a425b3a` ของเรา — คุ้มที่จะ `git fetch` ดู diff)
- คนที่ทำ upstream exynos850 คือกลุ่มเดียวกัน: Linaro (Klimov/Protsenko/Ambarus ทำ ACPM/PMU/TMU/hotplug) + Thomas Turner (a21s, GPU, pmOS) + Kaustabh Chakraborty (s2mu005 fuel gauge, exynos7870) — ทุกอย่างที่เราจะทำมีแนวโน้ม upstreamable

### 5.2 Thermal — patch ทำเสร็จแล้ว มีให้เลือก **2 series**

1. **Mateusz Majewski (Samsung), `[PATCH v4 0/7] Add initial Exynos850 support to the thermal driver`** (Mar 2026, lore linux-arm-kernel) — driver ระดับ register `samsung,exynos850-tmu`, ทดสอบจริงบน E850-96:
   ```
   tmuctrl_0: tmu@10070000 { compatible="samsung,exynos850-tmu"; reg=<0x10070000 0x800>;
     interrupts=<GIC_SPI 226 ...>; clocks=<&cmu_peri CLK_GOUT_BUSIF_TMU_PCLK>; #thermal-sensor-cells=<0>; }
   ```
   → ทำงานได้จริง (48°C load / 36-37°C idle); ไม่พึ่ง ACPM channel 9
2. **Alexey Klimov (Linaro), `[PATCH 0/3] acpm-tmu: Add Exynos850 support`** (Jul 2026, lore linux-pm) — extend `acpm-tmu.c` ด้วย `acpm_tmu_exynos850` (sensor groups: CPUCL0=sensor1, CPUCL1=sensor2, GPU=sensor3 — ตรงกับ vendor `LITTLE0/LITTLE1/G3D`); Exynos850 ACPM firmware มี IPC commands แค่ init/suspend/resume/read-temp ส่วนที่เหลือทำผ่าน MMIO
   - ⚠️ Klimov รายงานใน thread ว่า ACPM path รายงาน temp เพี้ยน (sensors อื่น ~10°C คงที่ / -29) → **register-level series (ข้อ 1) น่าเชื่อถือกว่าสำหรับเรา**
   - Tudor Ambarus มี series `acpm-tmu-fixes` (Oct 2026) rename driver เป็น `exynos-acpm-tmu` — ถ้าเลือก ACPM path ต้องตาม series นี้ด้วย
   - Patchew mirror: `patchew.org/linux/20260709-acpm-tmu-e850-support-v1-0-9fdd58b634af@linaro.org/`

### 5.3 CPU hotplug + cpuidle — patch มีแต่มี caveat

- **v7 series (Sep 2026):** `exynos-pmu` generalise + `exynos850-pmu.c` (CPU hotplug ผ่าน `pmu_intr_gen` block, เทส random off/on หลายชม. บน E850-96) + เพิ่ม `pmu_intr_gen` node ใน `exynos850.dtsi`
- **Idle states:** Klimov มี patch ใช้งานได้แต่ **"not upstreamable — PSCI call ต้อง shift argument เพราะ vendor firmware มี MPIDR เพี้ยน"** (reply ใน GPU thread, Aug 2026) — เขาเสนอแชร์ patch; ใช้ downstream ได้เลย ไม่ต้องคิดเอง → แปลว่า PSCI cpuidle ทำได้จริง แค่ต้องมี workaround
- GPU: Thomas Turner `[PATCH v2 0/2] Exynos850 GPU support` (Aug 2026) — เรามี panfrost/panvk ทำงานอยู่แล้ว แต่ series นี้คือทาง upstream

### 5.4 Modem — userspace load path มีอยู่ ไม่ต้องแก้ uniLoader เสมอไป

- **สำคัญ — แก้ finding 1.3:** บน Shannon ModAP มี path ที่ userspace อัปโหลดเฟิร์มแวร์ผ่าน `/dev/umts_boot0` (iodev ที่ `cpif` สร้างอยู่แล้วใน a21s dts): บน J700F/Exynos7870 (ModAP shmem เหมือนกัน) daemon `cbd` อัปโหลด BOOT/MAIN/NV ผ่าน `umts_boot0` — มี **open-source reimplementation `rebehzat/samsung-s310-cbd-replacement`** (S310, verified: boot/LTE/calls/SMS) เป็น reference ของ handshake protocol → ทางเลือก: รัน `cbd` blob จาก stock /vendor บน Arch Linux (เหมือน LineageOS) หรือใช้ reimpl แทนการแก้ uniLoader
- `eighty-twenty.org` (Sep 2020) — notes reverse-engineer การ boot Shannon modem (S7) ด้วย kernel interfaces ล้วน + python script reference (`cbd`/`rild` strace → /dev/umts_ipc0 + umts_boot0 + RADIO partition + `nv_data.bin` จาก EFS)
- `cpif` มี **version ใหม่กว่าใน Pixel/gs101 kernels** (`drivers/soc/google/cpif`, ยัง maintain บน 6.x) — base ที่ดีกว่า vendor 4.19 สำหรับ port ขึ้น 7.3
- Userspace stack: `libsamsung-ipc` official รองรับแค่อุปกรณ์เก่า (XMM626x) — แต่มี oFono plugin out-of-tree; ฝั่งใหม่กว่า Steffen Deusch ทำ Exynos 5300 (Pixel 9a) ทำงานบน pmOS ด้วย **ModemManager SIT plugin** (คนละ protocol กับเรา แต่พิสูจน์ว่าเส้นทาง MM ทำได้ — s318ap ใช้ SIPC ต้องไปทาง libsamsung-ipc/cbd)
- Firmware sources ที่ต้องมีบนเครื่อง: `modem.bin` (`RADIO`), `nv_data.bin` (EFS partition) — ทั้งคู่อยู่ใน eMMC อยู่แล้ว ไม่ต้อง extract

### 5.5 Battery / peripherals — สถานะ upstream

- `s2mu005-battery.c` upstream master ยัง bind แค่ `samsung,s2mu005-fuel-gauge` — เพิ่ม `samsung,s2mu106-fuelgauge` compatible เป็น patch เล็กที่ส่ง upstream ได้ (maintainer: Kaustabh Chakraborty — คนเดียวกับที่ทำ exynos7870); binding example ใน yaml ใช้ `fuel-gauge@3b` เหมือนเราเป๊ะ
- S2MU106 มี source ที่สองนอกจาก exynos850 tree: **LineageOS sm7325 kernel** (`a73xq`, Galaxy A73 — review.lineageos.org change 373716) มี `s2mu106_{core,irq,charger,fuelgauge}.c` ชุดเดียวกัน — cross-check register diff ได้
- ไม่มี upstream series สำหรับ S2MU106 MFD/charger/MUIC — ต้องเขียนเอง (แต่ FG ข้างบนครอบคลุม % แบตแล้ว)
- LM36274: `ti-lmu` + `leds-lm36274` + `lm363x-regulator` upstream ครบ (vendor node `lm36274@11` บน i2c_1)
- ไม่มีใครทำ `cpif`/`gnssif`/`muic`/sensorhub บน mainline — ส่วนนี้ต้องพอร์ตเองเต็มๆ

### 5.6 สรุป effort หลังรวมงานคนอื่น

| งาน | เดิม | ใหม่ | วิธี |
|---|---|---|---|
| TMU thermal | S–M | **XS** | cherry-pick Majewski v4 series (หรือ Klimov acpm-tmu) + DTS nodes |
| CPU hotplug/idle | M | **S** | cherry-pick v7 pmu series + ขอ idle-state patch จาก Klimov |
| Fuel gauge | S | **XS** | `s2mu005-battery.c` + compatible เดียว |
| Modem kernel | L | M–L | port จาก `drivers/soc/google/cpif` (6.x) แทน vendor 4.19 |
| Modem userspace | L | M | `cbd` blob/reimpl + libsamsung-ipc; เฟิร์มแวร์จาก eMMC โดยตรง |

---

## 7. ลำดับแนะนำ (ปรับจาก plan doc ตาม findings + งานคนอื่น)

| # | งาน | Effort | เหตุ |
|---|---|---|---|
| 1 | `schedutil` + `idle-states` DTS | **XS** | config + DTS; idle patch มีอยู่แล้ว (ข้อ 5.3) |
| 2 | `s2mu106-fuelgauge` ผ่าน `s2mu005-battery.c` + เปิด `&i2c_0` | **XS** | register map เหมือนกัน 100%; patch ส่ง upstream ได้ |
| 3 | `lm36274` backlight (`&i2c_1`) | **S** | upstream drivers พร้อม |
| 4 | Thermal: cherry-pick `samsung,exynos850-tmu` series (ข้อ 5.2) + trips | **XS–S** | patch ทำเสร็จแล้ว เหลือแค่ DTS + cooling map |
| 5 | CPU hotplug: cherry-pick exynos850-pmu v7 series | **S** | patch พร้อม, เทสแล้วบน E850-96 |
| 6 | Suspend-to-RAM ทดสอบทีละ subsystem | M | ขึ้นกับ PSCI/secure FW + driver resume |
| 7 | S2MU106 MUIC + charge control (จริงจัง) | M | ต้องออกแบบให้เข้ากับ extcon/typec mainline |
| 8 | `cpif`+`pmu-cp` port (base จาก `soc/google/cpif`) + `cbd` userspace | **M–L** | driver ใหญ่แต่ SMC/shmem path ชัดแล้ว + ไม่ต้องแตะ uniLoader ถ้าใช้ `umts_boot0` path |
| 9 | `libsamsung-ipc`/`oFono` + `gnome-calls` + VSS audio routing | L | SIPC5.0 + abox_vss พร้อม |
| 10 | `gnssif` (GPS) | M | pattern เดียวกับ cpif แต่ driver เล็กกว่า |

## 8. Open questions ที่เพิ่ม/แก้จาก ROADMAP

- [ ] **ช่องทางโหลด `modem.bin`:** `cbd`-style userspace upload ผ่าน `umts_boot0` (ไม่ต้องแตะ uniLoader) หรือ bootloader preload → `0xd0000000` — ลอง `cbd` blob จาก stock vendor บน Arch ก่อนตัดสินใจ
- [ ] exynos850 TMU: เลือก register-level (Majewski v4) หรือ ACPM (Klimov) — Klimov รายงาน ACPM อ่าน temp เพี้ยน → register-level น่าจะถูกกว่า; ทั้งคู่ต้องเทสบน a217f (E850-96 ≠ A21s trims)
- [ ] PSCI `CPU_SUSPEND`/`SYSTEM_SUSPEND` บน TEEGRIS ที่ resident + MPIDR arg-shift workaround (ขอ patch จาก Klimov) — ทดสอบ `rtcwake -m mem`/`s2idle` บนเครื่องจริง
- [ ] FG calibration: `s2mu005-battery.c` ไม่เขียน `battery_data` — เช็กว่า % SOC เพี้ยนไหมก่อนตัดสินใจย้าย calibration logic จาก vendor `s2mu106_fuelgauge.c`
- [ ] Rebase โอกาส: `samsungexynos850/linux` master ใหม่กว่า pin ของเรา (`8be82eac` มี `a217f-{3,4,6}gb.dts`) — diff ดูว่ามี driver/DTS ใหม่ที่เรายังไม่มี

## 9. Skeleton code ที่เขียนไว้แล้ว (ยังไม่ deploy)

เขียนโครงทั้งหมดใน repo แล้ว — compile ผ่าน (`make M=` ได้ `.ko` จริง) แต่**ยังไม่ได้เทสบนเครื่อง**:

| Artifact | Path | สถานะ |
|---|---|---|
| cpif driver skeleton | `drivers/cpif/` (`a21s-cpif.ko`) | POWER_ON/OFF/RESET + LOAD_CP_IMAGE + GET_CP_STATUS ทำงานในเชิงโค้ด; REQ_SECURITY/START_BOOTLOADER/COMPLETE ยัง `-EOPNOTSUPP` (รอ SMC id + mailbox) |
| PMU power sequences | `drivers/cpif/cpif_power.c` | แปล `pmucal_cp_exynos3830.h` เป็น regmap ops บน `pmu_system_controller` syscon + VGPIO wait `0x11a50000` |
| shmem/TOC parser | `drivers/cpif/cpif_shmem.c` | parse `cp_shmem/regions` + dump `cp_toc`/`cp_mem_map` หลังโหลด fw |
| SIPC stubs | `drivers/cpif/cpif_ipc.c` | `/dev/umts_ipc0`,`umts_ipc1`,`umts_rfs0` มี node แต่ I/O คืน `-EIO` |
| userspace loader | `tools/cbd/a21s-cbd.c` | เปิด `/dev/umts_boot0`, stream modem.bin/RADIO partition, รัน ioctl sequence |
| TMU skeleton | `drivers/tmu/exynos850-tmu.c` | probe + dump regs 0x10070000; get_temp/thermal-zones ยังไม่ทำ |
| DTS patch | `patches/linux/0005-a21s-power-modem-dts.patch` | `cp_rmem`@0xd0000000, hsi2c_0+s2mu106_fg@0x3b, hsi2c_1+lm36274@0x11, tmu node, `&cpu0`/`&cpu4` cooling-cells, cpif+cp_mailbox+cp_shmem (thermal-zones comment ไว้) |
| FG compatible | `patches/linux/0006-s2mu106-fuelgauge-compatible.patch` | เพิ่ม `samsung,s2mu106-fuel-gauge` ใน `s2mu005-battery.c` |
| defconfig | `configs/a21s_linux_defconfig` | `BATTERY_S2MU005=y`, `MFD_TI_LMU=m`, `LEDS_LM36274=m`, `REGULATOR_LM363X=m`, default governor → `schedutil` |

**ก่อน flash ครั้งแรก:** (1) `i2cdetect` เช็ก bus จริงของ s2mu106/lm36274 (patch เดา hsi2c_0/hsi2c_1 — vendor fragment target resolve ไม่ได้จาก source), (2) เช็ก PMU regs 0x3000+ เข้าถึงได้จาก non-secure world ไหม, (3) หา SMC_ID สำหรับ `REQ_SECURITY`, (4) modem.bin partition offset จริงจาก PIT

## 10. Camera — findings (A21s triple/quad camera)

Sensor list จาก vendor `a21s_eur_open_w00_r00.dts` `fragment@camera` + `camera-config`:

| Camera | Sensor | i2c addr | i2c ch | mclk ch | position |
|---|---|---|---|---|---|
| Rear main 48MP | Samsung **S5KGM2** (ISOCELL GM2) | `0x3d` | 0 | 0 | 0 (back) |
| Front 13MP | **HI1336** (Hynix) | `0xd3` | 1 | 1 | 1 (front) |
| Rear2 5MP | **GC02M1** (GalaxyCore) | `0xd6` | 2 | 2 | 2 |
| Rear3/macro 5MP | **GC02M1** (macro variant) | `0xd6` | 3 | 2 | 6 |
| Rear4 8MP UW | **S5K4HA** | `0x5a` (`sensor_id 0x27`) | 3 | 2 | 4 |

หมายเหตุ: `sensor_i2c_ch` 0-3 **ไม่ใช่ hsi2c** — เป็น camera i2c เฉพาะ → ไม่ชน s2mu106 @0x3b-0x3d บน hsi2c (ตอบข้อสงสัย bus ของ PMIC ได้ทางอ้อม: hsi2c_0 ยัง free สำหรับ s2mu106)

### 10.1 ISP hardware (Pablo ISP — downstream-only)

- `exynos_is@14440000` compatible `samsung,exynos-is` — ISP หลัก (Samsung "Pablo" ISP):
  - reg: `0x14430000-0x14480000` + `0x12ca0000`, IRQ GIC 147,156-161,168,171
  - `num_of_isp=1`, `num_of_3aa=2`, `num_of_mcsc=2`, `num_of_vra=1`
- `gdc@144C0000` — gamma/display correction, IRQ 146
- `exynos3830-isppre` syscon @ `0x14520500` (ISP pre-config)
- CSIS receivers: 3× `samsung,exynos-is-sensor` @ `0x14400000`/`14410000`/`14420000` (reg-names `csi`+`phy`, IRQ mode0_VC0-3)
- DPHY: 3× `samsung,mipi-phy-m0s4s4s2` @ `0x14490000`/`144a0000`/`144b0000`
- Power domain `pd-is@11862100`, camera sysmmus @ `0x14850000`/`14550000`/`14570000`
- `camera_rmem` reserved mem: `0xed210000`/`0x100000` (1 MiB — เล็กมาก น่าจะเป็น fw/shared structs)
- DVFS: `camera_default` EM scenario ผ่าน ACPM (เหมือน abox — ACPM infra เรามีอยู่แล้ว)

### 10.2 Driver — `drivers/media/platform/exynos/camera/` (~816 files)

- Stack: `is-core`/`is-devicemgr`/`is-groupmgr`/`is-framemgr`/`is-mem`/`is-pipe`/`is-sock`/`is-interface-wrap` + `is-video-3aa*` + `is-subdev-*` + `is-device-csi_v4`
- ischain versions: `is-v7_0_0`, `is-v8_0_0`, `is-v8_1_0`, `is-v8_10_0`, `is-v8_20_0` (compile หมด เลือกตอน runtime — Exynos850 น่าจะ v8_20_0)
- Sensor drivers: `sensor/module_framework/cis/is-cis-{gc02m1,hi1336,...}.c` + eeprom `mcd_v2/is-eeprom-*.h`
- ใช้ custom ioctl/socket interface คุยกับ userspace camera daemon — **ไม่ใช่ V4L2 media-controller path มาตรฐาน**

### 10.3 Userspace — ปัญหาใหญ่กว่า kernel

- Android ใช้ Samsung proprietary camera stack (`libslsi_cam`/sec-cam + camera app) คุยกับ is-* driver ผ่าน `is-sock`
- บน Arch/SteamOS ไม่มี equivalent — ทางเลือก: (ก) port kernel + เขียน userspace เองผ่าน is-sock protocol (reverse จาก `is-interface*.c`), (ข) V4L2-ize เฉพาะ path ที่ต้องการ (raw capture ผ่าน CSIS bypass ถ้า ISP รองรับ), (ค) libcamera + sensor drivers แยก — แต่ต้องมี ISP ที่ feed raw frames ออกมา
- Sensor init tables อยู่ใน `is-cis-*.c` (i2c register sequences) — reuse เป็น reference ได้ถ้าขับ sensor ตรง

### 10.4 Mainline/community status — **ไม่มีอะไรเลย**

- `exynos850.dtsi` mainline ไม่มี camera node ใดๆ (SoC support 2021 ใส่มาแค่ core)
- ไม่มี Pablo ISP driver ใน mainline — exynos4x12 `fimc-is` เป็นคนละ ISP gen ใช้ไม่ได้
- `samsungexynos850/linux` master/staging ไม่มี camera work
- pmOS `device-samsung-a21s` ไม่รองรับกล้อง (`firmware-samsung-a21s` มีแค่ `td4150_a21s.bin` ของ touchscreen)
- A21s ไม่มีใครทำกล้องบน mainline เลย — **เป็น peripheral ที่หนักสุดในบรรดาทั้งหมด** (หนักกว่า modem ด้วยซ้ำ เพราะ ISP+sensor+userspace stack ทั้งสามชั้น proprietary)

### 10.5 Effort estimate & ลำดับแนะนำ

- Kernel port `exynos/camera/` ทั้งก้อน: **XL** (816 files, Kconfig ยุ่ง, พึ่งพา vendor framework เยอะ) — หรือ minimal path: CSIS+sensor อย่างเดียวข้าม ISP (ถ้าถ่าย raw bayer ได้) = **L**
- Userspace: ต้องเขียนเองหรือ reverse is-sock protocol — **L–XL**
- **แนะนำ:** กล้องไว้หลังสุด (หลัง M6 modem + เสียงเสร็จ) — ถ้าจะทำจริง ควรเริ่มจาก minimal path: port `is-device-csi_v4` + sensor i2c drivers + ดูว่า CSIS→memory raw capture ทำได้ไหมก่อนค่อยตัดสินใจ ISP

## 11. Motion sensors / gyroscope — Samsung SSP (CHUB) sensor hub

### 11.1 Sensor chips บน A21s (จาก sensor dump ของเครื่องจริง + vendor driver list)

- **LSM6DSL** (STMicro) — accelerometer + gyroscope ในตัวเดียว (6-axis IMU) — ตัวหลักที่ตอบ "gyroscope"
- **AK09916C / AK09918C หรือ MMC5603** — magnetometer (compass)
- **GP2AP110S** (Sharp) — proximity
- light/ALS + grip **ISG5320A** (IMAGIS, สำหรับ grip/edge gesture)
- Virtual sensors (rotation vector, gravity, significant motion, step counter ฯลฯ) — คำนวณบน AP หรือใน CHUB fw

### 11.2 Architecture — ทุกอย่างผ่าน Samsung SSP/CHUB MCU

- Sensor จริง **ไม่ได้ต่อ AP โดยตรง** — ต่อกับ **CHUB** (sensor hub MCU) ซึ่งคุยกับ AP ผ่าน mailbox/shared memory:
  - `chub_rmem` reserved mem: `0xfd000000`/`0x50000`
  - wakeup source `MAILBOX_CHUB2AP`, `CHUB_WDT`
  - Firmware: `sensorhub/shub_nacho_a21s.bin` ("nacho" = platform codename ของ hub)
- มี `contexthub` (`samsung,exynos-nanohub`, fw `os.checked.bin`) อีกตัวสำหรับ context/scontext — คนละตัวกับ SSP
- Vendor kernel: `drivers/soc/samsung/exynos-ssp.c` (misc dev `/dev/ssp` ioctl shim "CMSSP") + `drivers/sensorhub/` (~40 files: `ssp_dev/comm/platform/sysfs/iio/firmware` + per-chip drivers ใน `accelerometer/gyroscope/magnetic/proximity/`)
- DT props `ssp-acc-position`, `ssp-mag-position`, `ssp-prox-thresh*` ฯลฯ ใน a21s dts → `ssp_dev.c` อ่านผ่าน `of_property_read_*` (platform device compatible `"ssp"` — node อยู่ใน base dts ที่ไม่ได้อยู่ใน sparse checkout)

### 11.3 Mainline status — transport มีแล้ว, sensor drivers ไม่มี

- **Upstream มี SSP transport + IIO clients แล้ว** (ตั้งแต่ ~2015, Exynos เก่า):
  - `drivers/iio/common/ssp_sensors/` — `ssp_dev.c`, `ssp_iio.c`, `ssp_iio_sensor.h`, `ssp_spi.c`
  - `drivers/iio/gyro/ssp_gyro_sensor.c`, `drivers/iio/accel/ssp_accel_sensor.c` — ครบใน tree 7.3 ของเราแล้ว
- **แต่** upstream SSP ผูกกับ **SPI transport** (`depends on SPI`) สำหรับ SoC เก่า — Exynos850 CHUB น่าจะใช้ mailbox/ACPM path (vendor `sensorhub_comms_write` ต้องตามไปดู transport จริง — open item)
- ไม่มี SSP node ใน `exynos850.dtsi`/`exynos850-a217f.dts` เลย — ต้องหา DT node จริง + transport regs
- Vendor per-chip drivers (`accel_lsm6dsl.c`, `gyro_lsm6dsl.c`, `magnetic_ak09916c.c`, `prox_gp2ap110s.c` ฯลฯ) เป็น vendor-only — mainline ไม่มี; แต่เป็น register-sequence drivers มาตรฐาน แปลงเป็น IIO drivers ตรงๆ ได้
- ไม่มีใครทำ SSP/CHUB บน Exynos850 mainline — pmOS ก็ไม่มี

### 11.4 Effort estimate

- หา transport path (ACPI? ACPM channel? mailbox regs) + DT node: **M** — ต้อง reverse จาก vendor `ssp_comm.c`/`ssp_platform.c` + syscon regs
- Port sensor drivers 4-5 ตัวเป็น IIO: **M** (register tables มีครบใน vendor source)
- Firmware `shub_nacho_a21s.bin`: ต้อง extract จาก vendor image (อยู่ใน vendor/modem partition?) — ถ้า CHUB บูตแล้วค้าง fw ไว้ อาจไม่ต้องดาวน์โหลดใหม่ทุก boot
- ทางเลือก minimal: ถ้า gyro/accel ไม่ critical สำหรับ SteamOS (ใช้แค่ auto-rotate + game orientation) → **defer**; ถ้าจะทำ → accel+gyro (LSM6DSL) ก่อน, mag/prox ตามหลัง
- Open questions: (1) transport จริง AP↔CHUB คืออะไร (ACPM channel?), (2) sensor อยู่บน bus ของ CHUB หรือ share กับ AP, (3) fw blob extract จาก partition ไหน, (4) node "ssp" จริงใน base DTB อยู่ไหน
