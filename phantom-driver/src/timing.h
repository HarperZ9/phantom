/*
 * timing.h — response timing normalization.
 *
 * Anti-fingerprinting systems detect spoofing by measuring IOCTL response
 * latency. Hooked queries typically add 50-500ms (or respond suspiciously
 * fast from a cache). The timing engine:
 *
 *   1. Calibrates by measuring real hardware response times during init
 *   2. Records mean and standard deviation per IOCTL type
 *   3. On interception, adds a jittered delay matching the measured distribution
 *
 * This makes spoofed responses indistinguishable from real ones by latency.
 */

#pragma once

#include "phantom.h"

#define PHANTOM_TIMING_SLOTS  16

typedef struct _PHANTOM_TIMING_SLOT {
    ULONG          IoControlCode;
    LARGE_INTEGER  MeanTicks;
    LARGE_INTEGER  StdDevTicks;
    ULONG          SampleCount;
    BOOLEAN        Calibrated;
} PHANTOM_TIMING_SLOT;

typedef struct _PHANTOM_TIMING_CTX {
    PHANTOM_TIMING_SLOT  Slots[PHANTOM_TIMING_SLOTS];
    ULONG                SlotCount;
    LARGE_INTEGER        PerformanceFrequency;
} PHANTOM_TIMING_CTX;

VOID     PhantomTimingInit(VOID);
VOID     PhantomTimingRecordSample(_In_ ULONG IoControlCode, _In_ LARGE_INTEGER ElapsedTicks);

NTSTATUS PhantomTimingCalibrate(
    _In_ PPHANTOM_FILTER_EXT FilterExt,
    _In_ ULONG IoControlCode
);

VOID PhantomTimingApplyDelay(
    _In_ ULONG IoControlCode,
    _In_ LARGE_INTEGER InterceptionStartTicks
);
