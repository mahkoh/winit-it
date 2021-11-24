#pragma once

#include <X11/Xdefs.h>
#include <stdint.h>

void video_init(pointer module);
void video_connect_second_monitor(uint32_t connected);
void video_get_info(uint32_t *second_crtc, uint32_t *first_output, uint32_t *second_output, uint32_t *small_mode_id, uint32_t *large_mode_id);

void input_init();
uint32_t input_new_keyboard();
void input_key_press(uint32_t keyboard, uint8_t key);
void input_key_release(uint32_t keyboard, uint8_t key);
void input_remove_device(uint32_t id);
