#pragma once

#include <X11/Xdefs.h>
#include <stdint.h>

void video_init(pointer module);
void video_connect_second_monitor();

void input_init();
uint32_t input_new_keyboard();
void input_key_press(uint32_t keyboard, uint8_t key);
void input_key_release(uint32_t keyboard, uint8_t key);
