#!/usr/bin/env bash
#
# mm-backend.sh — ModemManager call/SMS reference for rabbiz-shell Telephony.
#
# STATUS: REFERENCE / FUTURE — intentionally NOT wired into the shell yet.
#
# The A21s modem is an Exynos CP reached through the `cpif` kernel driver
# (Samsung IPC — /dev/umts_ipc0, NOT a USB/AT/QMI modem). Kernel-side port is
# Phase 4/M6 of devices/samsung/a21s/docs/a21s-power-battery-modem-plan.md. Once cpif lands and a
# userspace telephony stack can drive it, a privileged helper can exec this
# script (or speak org.freedesktop.ModemManager1 over D-Bus directly) and
# bridge Rabbiz.Services.Telephony via its `backend` hook:
#
#   Telephony.backend = {
#       dial(number), answer(), hangup(), sendSms(number, body),
#       sendDtmf(tone), refresh()
#   }
#
# PROTOCOL CAVEAT: Samsung IPC modems are normally driven via libsamsung-ipc /
# oFono rather than plain AT. If ModemManager lacks a plugin for the device,
# prefer `ipc-modem` / oFono (plan §4.2, option A/B) and map the same verbs.
#
# Usage (once a modem exists):
#   mm-backend.sh list                      # mmcli -L — pick a modem index
#   mm-backend.sh info [MODEM]              # modem / SIM / operator state
#   mm-backend.sh call <number> [MODEM]     # create voice call -> prints call obj
#   mm-backend.sh calls [MODEM]             # list voice call objects
#   mm-backend.sh accept <CALL>             # answer incoming call
#   mm-backend.sh hangup <CALL>             # terminate call
#   mm-backend.sh dtmf <CALL> <tones>       # send DTMF (e.g. "1*#")
#   mm-backend.sh sms <number> <text> [MODEM]
#   mm-backend.sh sms-list [MODEM]
#   mm-backend.sh sms-delete <SMS_IDX> [MODEM]
#
# Typical ModemManager versions used on Arch ARM ship these subcommands;
# voice support requires mmcli >= 1.18 (--voice-create-call).

set -euo pipefail

MODEM_DEFAULT="${MM_MODEM:-0}"

mmcli_check() {
    command -v mmcli >/dev/null 2>&1 || {
        echo "mmcli not found — install 'modemmanager' package" >&2
        exit 1
    }
}

cmd_list() {
    # Discover modems; first column is the index used as -m <n>.
    mmcli -L
}

cmd_info() {
    local m="${1:-$MODEM_DEFAULT}"
    mmcli_check
    mmcli -m "$m"
    echo "--- SIM/operator ---"
    mmcli -m "$m" --sim-get-imsi 2>/dev/null || true
    mmcli -m "$m" --3gpp-get-registration 2>/dev/null || true
    mmcli -m "$m" --3gpp-get-signal-quality 2>/dev/null || true
}

cmd_call() {
    # Create an outgoing voice call. Prints the call object path/index —
    # keep it for accept/hangup/dtmf subcommands.
    #   mmcli -m 0 --voice-create-call 'number=0812345678'
    local num="$1" m="${2:-$MODEM_DEFAULT}"
    mmcli_check
    mmcli -m "$m" --voice-create-call "number=${num}"
}

cmd_calls() {
    # List active voice calls and their object paths (used to resolve -o <n>).
    local m="${1:-$MODEM_DEFAULT}"
    mmcli_check
    mmcli -m "$m" --voice-list-calls
}

cmd_accept() {
    # Answer an incoming call object.
    local call="$1"
    mmcli_check
    mmcli -o "$call" --accept
}

cmd_hangup() {
    # Terminate a call object (works for outgoing + incoming).
    local call="$1"
    mmcli_check
    mmcli -o "$call" --hangup
}

cmd_dtmf() {
    # Send in-call tones, e.g. `mm-backend.sh dtmf 0 '1*#'`.
    local call="$1" tones="$2"
    mmcli_check
    mmcli -o "$call" --send-dtmf="$tones"
}

cmd_sms() {
    # Create + send an SMS. `mmcli --messaging-create-sms` only CREATES the
    # message object — it must then be sent with `mmcli -s <idx> --send`.
    # For backend use, capture the /org/.../SMS/<n> path from stdout and
    # re-invoke `mmcli -s <n> --send`.
    local num="$1" text="$2" m="${3:-$MODEM_DEFAULT}"
    mmcli_check
    mmcli -m "$m" --messaging-create-sms="text='${text}',number='${num}'"
}

cmd_sms_send() {
    local sms_idx="$1"
    mmcli_check
    mmcli -s "$sms_idx" --send
}

cmd_sms_list() {
    local m="${1:-$MODEM_DEFAULT}"
    mmcli_check
    mmcli -m "$m" --messaging-list-sms
}

cmd_sms_delete() {
    local sms_idx="$1" m="${2:-$MODEM_DEFAULT}"
    mmcli_check
    mmcli -m "$m" --messaging-delete-sms="$sms_idx"
}

# ---------------------------------------------------------------------------

main() {
    local sub="${1:-help}"
    shift || true
    case "$sub" in
        list)        cmd_list "$@" ;;
        info)        cmd_info "$@" ;;
        call)        cmd_call "$@" ;;
        calls)       cmd_calls "$@" ;;
        accept)      cmd_accept "$@" ;;
        hangup)      cmd_hangup "$@" ;;
        dtmf)        cmd_dtmf "$@" ;;
        sms)         cmd_sms "$@" ;;
        sms-send)    cmd_sms_send "$@" ;;
        sms-list)    cmd_sms_list "$@" ;;
        sms-delete)  cmd_sms_delete "$@" ;;
        help|-h|--help)
            sed -n '2,60p' "$0" | sed 's/^# \{0,1\}//'
            ;;
        *)
            echo "unknown subcommand: $sub (try --help)" >&2
            exit 2
            ;;
    esac
}

main "$@"
