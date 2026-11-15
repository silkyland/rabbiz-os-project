#!/bin/sh
# Apply SM-A217F "media-speaker" ABOX routing (from stock mixer_paths.xml)
# Usage on device: sh /tmp/abox_speaker.sh [card]
# Card defaults to 1 (Exynos3830-Madera). Run AFTER abox-snd + machine load.
CARD="${1:-1}"
C="amixer -c $CARD"

set -e

# --- Initial defaults (mixer_paths.xml header) ---
$C cset name='ABOX SIFS0 Width' 16
$C cset name='ABOX SIFS1 Width' 16
$C cset name='ABOX SIFS2 Width' 16
$C cset name='ABOX SIFS3 Width' 16
$C cset name='ABOX SIFS4 Width' 16
$C cset name='ABOX SIFS5 Width' 16

$C cset name='ABOX UAIF0 Width' 16
$C cset name='ABOX UAIF0 Channel' 2
$C cset name='ABOX UAIF0 Rate' 48000

$C cset name='ABOX UAIF1 Width' 16
$C cset name='ABOX UAIF1 Channel' 2
$C cset name='ABOX UAIF1 Rate' 48000
$C cset name='ABOX UAIF1 Extend BCLK' 1

$C cset name='ABOX UAIF2 Width' 16
$C cset name='ABOX UAIF2 Channel' 2
$C cset name='ABOX UAIF2 Rate' 8000

$C cset name='ABOX SPDY Rate' 40000

# Speaker path defaults: SPUS ASRC/OUT, SPUM, SIFM, NSRC, SIFS, UAIF SPK
$C cset name='ABOX SPUS ASRC0' 0
$C cset name='ABOX SPUS ASRC1' 0
$C cset name='ABOX SPUS ASRC2' 0
$C cset name='ABOX SPUS ASRC3' 1
$C cset name='ABOX SPUS ASRC4' 0
$C cset name='ABOX SPUS ASRC5' 0
$C cset name='ABOX SPUS ASRC6' 0
$C cset name='ABOX SPUS ASRC7' 0
$C cset name='ABOX SPUS ASRC8' 0
$C cset name='ABOX SPUS ASRC9' 0
$C cset name='ABOX SPUS ASRC10' 0
$C cset name='ABOX SPUS ASRC11' 0

$C cset name='ABOX SPUS OUT0' SIFS0
$C cset name='ABOX SPUS OUT1' SIFS0
$C cset name='ABOX SPUS OUT2' RESERVED
$C cset name='ABOX SPUS OUT3' RESERVED
$C cset name='ABOX SPUS OUT4' RESERVED
$C cset name='ABOX SPUS OUT5' RESERVED
$C cset name='ABOX SPUS OUT6' SIFS2
$C cset name='ABOX SPUS OUT7' SIFS1
$C cset name='ABOX SPUS OUT8' RESERVED
$C cset name='ABOX SPUS OUT9' RESERVED
$C cset name='ABOX SPUS OUT10' RESERVED
$C cset name='ABOX SPUS OUT11' RESERVED

$C cset name='ABOX UAIF0 SPK' RESERVED
$C cset name='ABOX UAIF1 SPK' RESERVED
$C cset name='ABOX UAIF2 SPK' RESERVED

$C cset name='ABOX SPUM ASRC0' 0
$C cset name='ABOX SPUM ASRC1' 0
$C cset name='ABOX SPUM ASRC2' 0
$C cset name='ABOX SPUM ASRC3' 0
$C cset name='ABOX SPUM ASRC4' 0

$C cset name='ABOX SIFM0' WDMA
$C cset name='ABOX SIFM1' WDMA
$C cset name='ABOX SIFM2' WDMA
$C cset name='ABOX SIFM3' WDMA
$C cset name='ABOX SIFM4' WDMA

$C cset name='ABOX NSRC0' RESERVED
$C cset name='ABOX NSRC1' RESERVED
$C cset name='ABOX NSRC2' RESERVED
$C cset name='ABOX NSRC3' RESERVED
$C cset name='ABOX NSRC4' RESERVED

$C cset name='ABOX SIFS1' RESERVED
$C cset name='ABOX SIFS2' RESERVED
$C cset name='ABOX SIFS3' RESERVED
$C cset name='ABOX SIFS4' RESERVED
$C cset name='ABOX SIFS5' RESERVED

# --- media-speaker path (ABOX/DSP routing only) ---
# NOTE: codec controls (Speaker Mute, DAC, HP/EP) are AUD3004X SPEEDY/ACPM
# registers, NOT ALSA kcontrols on this card — handled separately in Phase B.
$C cset name='ABOX UAIF1 SPK' SIFS0
$C cset name='ABOX SPUS OUT3' SIFS0
$C cset name='ABOX SPUS ASRC3' 1
$C cset name='ABOX RDMA3_A' BD_MIXER
$C cset name='ABOX BD_SRCCOM0_A' VPCMOUT_DAI0
$C cset name='ABOX BD_SRCCOM1_A' VPCMOUT_DAI1
$C cset name='ABOX BD_SRCCOM2_A' VPCMOUT_DAI2
$C cset name='ABOX BD_SRCCOM3_A' VPCMOUT_DAI3

echo "media-speaker routing applied on card $CARD"
