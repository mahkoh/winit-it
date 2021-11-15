#include <xorg-server.h>
#include <X11/Xdefs.h>
#include <xf86Xinput.h>
#include <stdint.h>
#include "winit.h"

#define DRIVER_VERSION 1
#define DRIVER_NAME "winit_input"

typedef enum {
  TyKeyboard = 1,
  TyMouse,
} Type;

typedef struct Device {
  struct Device *next;
  struct Device **prev_next;
  Type type;
  InputInfoPtr device;
} Device;

static Device *devices;
static Type current_type;

static int device_control(DeviceIntPtr dev, int what) {
  InputInfoPtr pInfo = dev->public.devicePrivate;
  Device *device = pInfo->private;

  switch (what) {
  case DEVICE_INIT:
    switch (device->type) {
    case TyKeyboard:
      assert(InitKeyboardDeviceStruct(dev, NULL, NULL, NULL));
      break;
    case TyMouse:
      break;
    }
  case DEVICE_ON:
  case DEVICE_OFF:
  case DEVICE_CLOSE:
    return Success;
  default:
    return BadValue;
  }
}

static int pre_init(InputDriverPtr drv, InputInfoPtr pInfo, int flags) {
  Device *device = calloc(sizeof(*device), 1);
  device->device = pInfo;
  device->type = current_type;
  pInfo->private = device;
  switch (current_type) {
  case TyKeyboard:
    pInfo->type_name = XI_KEYBOARD;
    break;
  case TyMouse:
    pInfo->type_name = XI_MOUSE;
    break;
  default:
    assert(0 && "Invalid type");
  }
  pInfo->device_control = device_control;

  device->prev_next = &devices;
  while (*device->prev_next) {
    device->prev_next = &(*device->prev_next)->next;
  }
  *device->prev_next = device;

  return Success;
}

static void un_init(InputDriverPtr drv, InputInfoPtr pInfo, int flags) {
  Device *device = pInfo->private;
  pInfo->private = NULL;
  *device->prev_next = device->next;
  if (device->next) {
    device->next->prev_next = device->prev_next;
  }
  free(device);
}

void input_init(pointer module) {
  static InputDriverRec driver = {
      .driverVersion = DRIVER_VERSION,
      .driverName = DRIVER_NAME,
      .PreInit = pre_init,
      .UnInit = un_init,
  };
  xf86AddInputDriver(&driver, module, 0);
}

static uint32_t input_new(const char *prefix) {
  static int next_input_id = 1;

  InputOption *options = NULL;
  char *name;
  uint32_t id = next_input_id++;
  asprintf(&name, "%s%u", prefix, id);
  options = input_option_new(options, "driver", strdup(DRIVER_NAME));
  options = input_option_new(options, "name", name);
  options = input_option_new(options, "floating", strdup("1"));
  DeviceIntPtr dev;
  assert(!NewInputDeviceRequest(options, NULL, &dev));
  input_option_free_list(&options);
  return (uint32_t)dev->id;
}

uint32_t input_new_keyboard() {
  current_type = TyKeyboard;
  return input_new("keyboard");
}

#define MIN_KEYCODE 8

static Device *get_device(uint32_t id) {
  Device *device = devices;
  while (device) {
    if (device->device->dev->id == id) {
      break;
    }
    device = device->next;
  }
  assert(device);
  return device;
}

static Device *get_keyboard(uint32_t keyboard) {
  Device *device = get_device(keyboard);
  assert(device->type == TyKeyboard);
  return device;
}

void input_key_press(uint32_t keyboard, uint8_t key) {
  Device *device = get_keyboard(keyboard);
  xf86PostKeyboardEvent(device->device->dev, key + MIN_KEYCODE, 1);
}

void input_key_release(uint32_t keyboard, uint8_t key) {
  Device *device = get_keyboard(keyboard);
  xf86PostKeyboardEvent(device->device->dev, key + MIN_KEYCODE, 0);
}

void input_remove_device(uint32_t id) {
  Device *device = get_device(id);
  DeleteInputDeviceRequest(device->device->dev);
}
