/* Build-time ABI constants only. No C driver code is compiled. */
#include <linux/hid.h>

/* The Rust transport uses typed pointer loads and fixed callback signatures.
 * Regenerating offsets alone would not catch a field or callback type change.
 * These checks run in clang/bindgen; no C driver object is linked. */
#define PH_FIELD(type, member, expected) \
    _Static_assert(__builtin_types_compatible_p(__typeof__(((type *)0)->member), expected), \
                   "APS ABI changed: " #type "." #member)
PH_FIELD(struct hid_device, bus, __u16);
PH_FIELD(struct hid_device, vendor, __u32);
PH_FIELD(struct hid_device, product, __u32);
PH_FIELD(struct hid_device, rsize, unsigned int);
PH_FIELD(struct hid_device, dev_rsize, unsigned int);
PH_FIELD(struct hid_device, rdesc, const __u8 *);
PH_FIELD(struct hid_device, dev_rdesc, const __u8 *);
PH_FIELD(struct hid_device, quirks, unsigned int);
PH_FIELD(struct hid_device, claimed, unsigned int);
PH_FIELD(struct input_dev, name, const char *);
PH_FIELD(struct input_dev, phys, const char *);
PH_FIELD(struct input_dev, id.bustype, __u16);
PH_FIELD(struct hid_driver, probe, int (*)(struct hid_device *, const struct hid_device_id *));
PH_FIELD(struct hid_driver, remove, void (*)(struct hid_device *));
PH_FIELD(struct hid_driver, raw_event, int (*)(struct hid_device *, struct hid_report *, u8 *, int));
PH_FIELD(struct hid_driver, reset_resume, int (*)(struct hid_device *));
PH_FIELD(struct input_handler, filter, bool (*)(struct input_handle *, unsigned int, unsigned int, int));
PH_FIELD(struct input_handler, match, bool (*)(struct input_handler *, struct input_dev *));
PH_FIELD(struct input_handler, connect, int (*)(struct input_handler *, struct input_dev *, const struct input_device_id *));
PH_FIELD(struct input_handler, disconnect, void (*)(struct input_handle *));
_Static_assert(sizeof(enum hid_report_type) == 4 && sizeof(enum hid_class_request) == 4,
               "APS HID request enum ABI changed");
_Static_assert(__builtin_types_compatible_p(__typeof__(&hid_hw_raw_request),
               int (*)(struct hid_device *, unsigned char, __u8 *, size_t,
                       enum hid_report_type, enum hid_class_request)), "APS HID request ABI changed");
_Static_assert(__builtin_offsetof(struct hid_device_id, bus) == 0 &&
               __builtin_offsetof(struct hid_device_id, group) == 2 &&
               __builtin_offsetof(struct hid_device_id, vendor) == 4 &&
               __builtin_offsetof(struct hid_device_id, product) == 8 &&
               __builtin_offsetof(struct hid_device_id, driver_data) == 16,
               "APS HID ID layout changed");
#undef PH_FIELD
enum {
    PH_HID_SIZE = sizeof(struct hid_device),
    PH_HID_DEV = __builtin_offsetof(struct hid_device, dev),
    PH_HID_BUS = __builtin_offsetof(struct hid_device, bus),
    PH_HID_VENDOR = __builtin_offsetof(struct hid_device, vendor),
    PH_HID_PRODUCT = __builtin_offsetof(struct hid_device, product),
    PH_HID_RDESC = __builtin_offsetof(struct hid_device, rdesc),
    PH_HID_RSIZE = __builtin_offsetof(struct hid_device, rsize),
    PH_HID_DEV_RDESC = __builtin_offsetof(struct hid_device, dev_rdesc),
    PH_HID_DEV_RSIZE = __builtin_offsetof(struct hid_device, dev_rsize),
    PH_HID_QUIRKS = __builtin_offsetof(struct hid_device, quirks),
    PH_HID_CLAIMED = __builtin_offsetof(struct hid_device, claimed),
    PH_HID_INPUT_PER_APP = HID_QUIRK_INPUT_PER_APP,
    PH_HID_CONNECT_DEFAULT = HID_CONNECT_DEFAULT,
    PH_HID_CLAIMED_INPUT = HID_CLAIMED_INPUT,
    PH_DRIVER_SIZE = sizeof(struct hid_driver),
    PH_DRIVER_ALIGN = __alignof__(struct hid_driver),
    PH_DRIVER_NAME = __builtin_offsetof(struct hid_driver, name),
    PH_DRIVER_IDS = __builtin_offsetof(struct hid_driver, id_table),
    PH_DRIVER_PROBE = __builtin_offsetof(struct hid_driver, probe),
    PH_DRIVER_REMOVE = __builtin_offsetof(struct hid_driver, remove),
    PH_DRIVER_RAW_EVENT = __builtin_offsetof(struct hid_driver, raw_event),
    PH_DRIVER_RESET_RESUME = __builtin_offsetof(struct hid_driver, reset_resume),
    PH_ID_SIZE = sizeof(struct hid_device_id),
    PH_DEVICE_MUTEX = __builtin_offsetof(struct device, mutex),
    PH_INPUT_NAME = __builtin_offsetof(struct input_dev, name),
    PH_INPUT_PHYS = __builtin_offsetof(struct input_dev, phys),
    PH_INPUT_BUS = __builtin_offsetof(struct input_dev, id.bustype),
    PH_INPUT_VENDOR = __builtin_offsetof(struct input_dev, id.vendor),
    PH_INPUT_PRODUCT = __builtin_offsetof(struct input_dev, id.product),
    PH_INPUT_VERSION = __builtin_offsetof(struct input_dev, id.version),
    PH_INPUT_HANDLER_SIZE = sizeof(struct input_handler),
    PH_INPUT_HANDLER_FILTER = __builtin_offsetof(struct input_handler, filter),
    PH_INPUT_HANDLER_MATCH = __builtin_offsetof(struct input_handler, match),
    PH_INPUT_HANDLER_CONNECT = __builtin_offsetof(struct input_handler, connect),
    PH_INPUT_HANDLER_DISCONNECT = __builtin_offsetof(struct input_handler, disconnect),
    PH_INPUT_HANDLER_NAME = __builtin_offsetof(struct input_handler, name),
    PH_INPUT_HANDLER_IDS = __builtin_offsetof(struct input_handler, id_table),
    PH_INPUT_HANDLE_SIZE = sizeof(struct input_handle),
    PH_INPUT_HANDLE_NAME = __builtin_offsetof(struct input_handle, name),
    PH_INPUT_HANDLE_DEVICE = __builtin_offsetof(struct input_handle, dev),
    PH_INPUT_HANDLE_HANDLER = __builtin_offsetof(struct input_handle, handler),
    PH_INPUT_ID_SIZE = sizeof(struct input_device_id),
    PH_INPUT_ID_FLAGS = __builtin_offsetof(struct input_device_id, flags),
    PH_INPUT_ID_BUS = __builtin_offsetof(struct input_device_id, bustype),
    PH_INPUT_MATCH_BUS = INPUT_DEVICE_ID_MATCH_BUS,
};
