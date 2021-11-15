#include <xorg-server.h>
#include <xf86Module.h>
#include <xf86.h>
#include "winit.h"
#include <stdbool.h>
#include <sys/un.h>
#include <unistd.h>
#include <assert.h>

static XF86ModuleVersionInfo winit_version = {
    .modname = "winit",
    .xf86version = XORG_VERSION_CURRENT,
};

enum MessageType {
  MT_NONE,
  MT_CREATE_KEYBOARD,
  MT_CREATE_KEYBOARD_REPLY,
  MT_KEY_PRESS,
  MT_KEY_RELEASE,
  MT_REMOVE_DEVICE,
};

typedef struct {
  uint32_t type;
  uint32_t id;
} CreateKeyboardReply;

typedef union {
  uint32_t type;
  struct {
    uint32_t type;
    uint32_t id;
    uint32_t key;
  } key_press;
  struct {
    uint32_t type;
    uint32_t id;
  } remove_device;
} Message;

static void handle_message(int fd, void *closure) {
  Message message;
  assert(read(fd, &message, sizeof(message)) > 0);
  switch (message.type) {
  case MT_CREATE_KEYBOARD: {
    uint32_t id = input_new_keyboard();
    CreateKeyboardReply reply = {
        .type = MT_CREATE_KEYBOARD_REPLY,
        .id = id,
    };
    assert(write(fd, &reply, sizeof(reply)) > 0);
    break;
  }
  case MT_KEY_PRESS:
    input_key_press(message.key_press.id, message.key_press.key);
    break;
  case MT_KEY_RELEASE:
    input_key_release(message.key_press.id, message.key_press.key);
    break;
  case MT_REMOVE_DEVICE:
    input_remove_device(message.remove_device.id);
    break;
  default:
    LogMessage(X_ERROR, "Invalid message type %u\n", message.type);
    assert(0 && "Invalid message type");
  }
}

static pointer winit_setup(pointer module, pointer opts, int *errmaj,
                           int *errmin) {
  static bool done = false;

  if (done) {
    if (errmaj) {
      *errmaj = LDR_ONCEONLY;
    }
    return NULL;
  }
  done = true;

  video_init(module);
  input_init(module);

  char *socknum = getenv("WINIT_IT_SOCKET");
  assert(socknum);
  int sock = atoi(socknum);
  xf86AddGeneralHandler(sock, handle_message, NULL);

  return (void *)1;
}

__attribute__((visibility("default"))) XF86ModuleData winitModuleData = {
    .vers = &winit_version,
    .setup = winit_setup,
};
