#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>

#include "codex-app-gpui-native-input-client-protocol.h"

struct pointer_state {
    struct zwlr_virtual_pointer_manager_v1 *manager;
    struct zwlr_virtual_pointer_v1 *pointer;
};

static void registry_global(void *data, struct wl_registry *registry, uint32_t name,
                            const char *interface, uint32_t version) {
    struct pointer_state *state = data;
    if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
        uint32_t bind_version = version < 2 ? version : 2;
        state->manager = wl_registry_bind(registry, name,
                                          &zwlr_virtual_pointer_manager_v1_interface,
                                          bind_version);
    }
}

static void registry_global_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {
    .global = registry_global,
    .global_remove = registry_global_remove,
};

static int parse_number(const char *text, double *value) {
    char *end = NULL;
    errno = 0;
    *value = strtod(text, &end);
    return errno == 0 && end != text && *end == '\0';
}

static void short_pause(void) {
    struct timespec duration = { .tv_sec = 0, .tv_nsec = 50000000L };
    while (nanosleep(&duration, &duration) < 0 && errno == EINTR) {
    }
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s DX DY\n", argv[0]);
        return 2;
    }

    double dx;
    double dy;
    if (!parse_number(argv[1], &dx) || !parse_number(argv[2], &dy)) {
        fprintf(stderr, "DX and DY must be finite numbers\n");
        return 2;
    }

    struct wl_display *display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "could not connect to Wayland\n");
        return 1;
    }
    struct pointer_state state = {0};
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, &state);
    if (wl_display_roundtrip(display) < 0 || !state.manager) {
        fprintf(stderr, "compositor lacks zwlr_virtual_pointer_manager_v1\n");
        wl_display_disconnect(display);
        return 1;
    }
    state.pointer = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(state.manager, NULL);
    if (!state.pointer) {
        fprintf(stderr, "could not create virtual pointer\n");
        zwlr_virtual_pointer_manager_v1_destroy(state.manager);
        wl_display_disconnect(display);
        return 1;
    }

    uint32_t time = 1;
    zwlr_virtual_pointer_v1_motion(state.pointer, time++, wl_fixed_from_double(dx), wl_fixed_from_double(dy));
    zwlr_virtual_pointer_v1_frame(state.pointer);
    if (wl_display_roundtrip(display) < 0) return 1;
    zwlr_virtual_pointer_v1_button(state.pointer, time++, 0x110, 1);
    zwlr_virtual_pointer_v1_frame(state.pointer);
    if (wl_display_roundtrip(display) < 0) return 1;
    short_pause();
    zwlr_virtual_pointer_v1_button(state.pointer, time++, 0x110, 0);
    zwlr_virtual_pointer_v1_frame(state.pointer);
    int result = wl_display_roundtrip(display) < 0 ? 1 : 0;
    zwlr_virtual_pointer_v1_destroy(state.pointer);
    zwlr_virtual_pointer_manager_v1_destroy(state.manager);
    wl_display_disconnect(display);
    return result;
}
