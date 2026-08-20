/*
 * timing.c — response timing normalization engine.
 *
 * Measures real hardware IOCTL response times, records statistics, and
 * applies jittered delays to spoofed responses so they match the measured
 * distribution. Uses KeQueryPerformanceCounter for sub-microsecond precision.
 */

#include "timing.h"

static PHANTOM_TIMING_CTX g_TimingCtx;

VOID PhantomTimingInit(VOID)
{
    RtlZeroMemory(&g_TimingCtx, sizeof(g_TimingCtx));
    KeQueryPerformanceCounter(&g_TimingCtx.PerformanceFrequency);
}

/*
 * Find or allocate a timing slot for this IOCTL code.
 */
static PHANTOM_TIMING_SLOT* FindOrAllocSlot(ULONG IoControlCode)
{
    ULONG i;

    for (i = 0; i < g_TimingCtx.SlotCount; i++) {
        if (g_TimingCtx.Slots[i].IoControlCode == IoControlCode) {
            return &g_TimingCtx.Slots[i];
        }
    }

    if (g_TimingCtx.SlotCount >= PHANTOM_TIMING_SLOTS) {
        return NULL;
    }

    i = g_TimingCtx.SlotCount++;
    RtlZeroMemory(&g_TimingCtx.Slots[i], sizeof(PHANTOM_TIMING_SLOT));
    g_TimingCtx.Slots[i].IoControlCode = IoControlCode;
    return &g_TimingCtx.Slots[i];
}

/*
 * Record a timing sample from a real (unintercepted) IOCTL completion.
 * Uses Welford's online algorithm for running mean and variance.
 */
VOID PhantomTimingRecordSample(
    _In_ ULONG IoControlCode,
    _In_ LARGE_INTEGER ElapsedTicks
)
{
    PHANTOM_TIMING_SLOT* slot = FindOrAllocSlot(IoControlCode);
    LONGLONG delta, delta2;
    LONGLONG oldMean;

    if (!slot) return;

    slot->SampleCount++;

    if (slot->SampleCount == 1) {
        slot->MeanTicks.QuadPart = ElapsedTicks.QuadPart;
        slot->StdDevTicks.QuadPart = 0;
        slot->Calibrated = TRUE;
        return;
    }

    oldMean = slot->MeanTicks.QuadPart;
    delta = ElapsedTicks.QuadPart - oldMean;
    slot->MeanTicks.QuadPart = oldMean + delta / (LONGLONG)slot->SampleCount;

    delta2 = ElapsedTicks.QuadPart - slot->MeanTicks.QuadPart;

    /*
     * Approximate stddev from running variance.
     * We store sqrt(variance) as stddev in ticks for the delay calculation.
     * Using integer sqrt approximation.
     */
    {
        LONGLONG variance = (delta * delta2) / (LONGLONG)slot->SampleCount;
        if (variance < 0) variance = -variance;
        slot->StdDevTicks.QuadPart = IntegerSqrt(variance);
    }
}

/*
 * Integer square root via Newton's method.
 */
static LONGLONG IntegerSqrt(LONGLONG value)
{
    LONGLONG x, x1;

    if (value <= 0) return 0;
    if (value == 1) return 1;

    x = value;
    x1 = (x + 1) / 2;

    while (x1 < x) {
        x = x1;
        x1 = (x + value / x) / 2;
    }

    return x;
}

/*
 * Apply a timing delay to make a spoofed response match the calibrated
 * response time distribution.
 *
 * If the interception took less time than the calibrated mean, spin-wait
 * for the remaining duration plus jitter. If it already took longer
 * (unlikely for a simple buffer rewrite), skip the delay.
 */
NTSTATUS PhantomTimingCalibrate(
    _In_ PPHANTOM_FILTER_EXT FilterExt,
    _In_ ULONG IoControlCode
)
{
    LARGE_INTEGER before, after, elapsed;
    IO_STATUS_BLOCK ioStatus;
    KEVENT event;
    PIRP calibrationIrp;
    NTSTATUS status;
    ULONG i;
    UCHAR dummyBuffer[256];

    if (!FilterExt || !FilterExt->LowerDevice) {
        return STATUS_INVALID_PARAMETER;
    }

    #define CALIBRATION_SAMPLES 8

    KeInitializeEvent(&event, NotificationEvent, FALSE);

    for (i = 0; i < CALIBRATION_SAMPLES; i++) {
        RtlZeroMemory(dummyBuffer, sizeof(dummyBuffer));

        calibrationIrp = IoBuildDeviceIoControlRequest(
            IoControlCode,
            FilterExt->LowerDevice,
            dummyBuffer,
            sizeof(dummyBuffer),
            dummyBuffer,
            sizeof(dummyBuffer),
            FALSE,
            &event,
            &ioStatus
        );

        if (!calibrationIrp) {
            return STATUS_INSUFFICIENT_RESOURCES;
        }

        KeClearEvent(&event);
        before = KeQueryPerformanceCounter(NULL);

        status = IoCallDriver(FilterExt->LowerDevice, calibrationIrp);

        if (status == STATUS_PENDING) {
            KeWaitForSingleObject(&event, Executive, KernelMode, FALSE, NULL);
        }

        after = KeQueryPerformanceCounter(NULL);
        elapsed.QuadPart = after.QuadPart - before.QuadPart;

        PhantomTimingRecordSample(IoControlCode, elapsed);
    }

    return STATUS_SUCCESS;
}

VOID PhantomTimingApplyDelay(
    _In_ ULONG IoControlCode,
    _In_ LARGE_INTEGER InterceptionStartTicks
)
{
    PHANTOM_TIMING_SLOT* slot;
    LARGE_INTEGER now;
    LONGLONG elapsed, target, jitter;
    LARGE_INTEGER delay;
    ULONG i;

    slot = NULL;
    for (i = 0; i < g_TimingCtx.SlotCount; i++) {
        if (g_TimingCtx.Slots[i].IoControlCode == IoControlCode) {
            slot = &g_TimingCtx.Slots[i];
            break;
        }
    }

    if (!slot || !slot->Calibrated || slot->SampleCount < 3) {
        return;
    }

    now = KeQueryPerformanceCounter(NULL);
    elapsed = now.QuadPart - InterceptionStartTicks.QuadPart;

    /*
     * Target = mean + uniform jitter in [-stddev, +stddev].
     * Using KeQueryPerformanceCounter low bits as entropy source —
     * not cryptographic, but sufficient for timing jitter.
     */
    jitter = 0;
    if (slot->StdDevTicks.QuadPart > 0) {
        LARGE_INTEGER entropy = KeQueryPerformanceCounter(NULL);
        LONGLONG range = 2 * slot->StdDevTicks.QuadPart;
        jitter = (entropy.QuadPart % range) - slot->StdDevTicks.QuadPart;
    }

    target = slot->MeanTicks.QuadPart + jitter;

    if (elapsed >= target) {
        return;
    }

    /*
     * Convert remaining ticks to 100-nanosecond units for KeDelayExecutionThread.
     * Negative value = relative delay.
     */
    {
        LONGLONG remainingTicks = target - elapsed;
        LONGLONG hundred_ns = (remainingTicks * 10000000LL) /
                              g_TimingCtx.PerformanceFrequency.QuadPart;

        if (hundred_ns > 0 && hundred_ns < 50000000LL) { /* cap at 5 seconds */
            delay.QuadPart = -(LONGLONG)hundred_ns;
            KeDelayExecutionThread(KernelMode, FALSE, &delay);
        }
    }
}
