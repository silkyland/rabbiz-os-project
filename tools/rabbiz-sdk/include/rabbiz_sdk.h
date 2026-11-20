#ifndef RABBIZ_SDK_H
#define RABBIZ_SDK_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define RABBIZ_SDK_ABI_VERSION 1u

typedef struct RabbizColor {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} RabbizColor;

typedef struct RabbizRect {
    int32_t x;
    int32_t y;
    int32_t w;
    int32_t h;
} RabbizRect;

typedef struct RabbizSize {
    int32_t w;
    int32_t h;
} RabbizSize;

typedef struct RabbizPoint {
    int32_t x;
    int32_t y;
} RabbizPoint;

typedef struct RabbizThemeC {
    RabbizColor bg;
    RabbizColor canvas;
    RabbizColor surface1;
    RabbizColor surface2;
    RabbizColor surface3;
    RabbizColor text;
    RabbizColor text2;
    RabbizColor text3;
    RabbizColor text_on_accent;
    RabbizColor accent;
    RabbizColor danger;
    RabbizColor success;
    RabbizColor warning;
    RabbizColor line;
} RabbizThemeC;

typedef enum RabbizEventKind {
    RABBIZ_EV_TOUCH_DOWN = 1,
    RABBIZ_EV_TOUCH_MOVE = 2,
    RABBIZ_EV_TOUCH_UP = 3,
    RABBIZ_EV_KEY = 4,
    RABBIZ_EV_TEXT_COMMIT = 5,
    RABBIZ_EV_TEXT_BACKSPACE = 6,
    RABBIZ_EV_TICK = 7,
    RABBIZ_EV_SCROLL = 8
} RabbizEventKind;

typedef struct RabbizEventC {
    uint32_t kind;
    int32_t id;
    int32_t x;
    int32_t y;
    uint32_t keysym;
    uint32_t count;
    uint32_t dt_ms;
    const char *utf8;
} RabbizEventC;

typedef struct RabbizFrameResultC {
    bool dirty;
    bool animating;
    bool quit;
} RabbizFrameResultC;

typedef struct RabbizCanvasC RabbizCanvasC;
typedef struct RabbizTextInputC RabbizTextInputC;

typedef RabbizFrameResultC (*RabbizOnEventFn)(
    void *userdata,
    const RabbizEventC *event,
    RabbizTextInputC *text_input
);

typedef void (*RabbizOnRenderFn)(
    void *userdata,
    RabbizCanvasC *canvas,
    const RabbizThemeC *theme
);

typedef struct RabbizAppCallbacksC {
    const char *app_id;
    const char *title;
    void *userdata;
    RabbizOnEventFn on_event;
    RabbizOnRenderFn on_render;
} RabbizAppCallbacksC;

uint32_t rabbiz_sdk_abi_version(void);
void rabbiz_theme_load(RabbizThemeC *out);

RabbizSize rabbiz_canvas_size(const RabbizCanvasC *c);
void rabbiz_canvas_clear(RabbizCanvasC *c, RabbizColor color);
void rabbiz_canvas_fill_rect(RabbizCanvasC *c, RabbizRect rect, RabbizColor color);
void rabbiz_canvas_fill_rounded_rect(RabbizCanvasC *c, RabbizRect rect, float radius, RabbizColor color);
RabbizRect rabbiz_canvas_draw_text(
    RabbizCanvasC *c,
    const char *utf8,
    int32_t x,
    int32_t y,
    float size_px,
    RabbizColor color
);

void rabbiz_text_input_activate(RabbizTextInputC *ti, uint32_t purpose);
void rabbiz_text_input_deactivate(RabbizTextInputC *ti);

size_t rabbiz_storage_data_dir(const char *app_id, char *buf, size_t cap);

int32_t rabbiz_app_run(const RabbizAppCallbacksC *cb);
size_t rabbiz_app_render_headless(
    const RabbizAppCallbacksC *cb,
    int32_t width,
    int32_t height,
    uint8_t *out_rgba,
    size_t cap
);

#ifdef __cplusplus
}
#endif

#endif /* RABBIZ_SDK_H */
