#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef enum ReturnCode {
  SUCCESS = 0,
  FAIL,
} ReturnCode;

typedef struct CContext {
  const char *db_path;
  const char *cache_dir;
} CContext;

typedef enum COption_CContext_Tag {
  None_CContext,
  Some_CContext,
} COption_CContext_Tag;

typedef struct COption_CContext {
  COption_CContext_Tag tag;
  union {
    struct {
      struct CContext some;
    };
  };
} COption_CContext;

typedef struct Return_CContext {
  struct COption_CContext data;
  enum ReturnCode code;
} Return_CContext;

typedef struct Return_CContext ReturnContext;

typedef enum COption_____c_char_Tag {
  None_____c_char,
  Some_____c_char,
} COption_____c_char_Tag;

typedef struct COption_____c_char {
  COption_____c_char_Tag tag;
  union {
    struct {
      char *some;
    };
  };
} COption_____c_char;

typedef struct Return_____c_char {
  struct COption_____c_char data;
  enum ReturnCode code;
} Return_____c_char;

typedef struct Return_____c_char ReturnStr;

ReturnContext get_context(const char *db_path, const char *cache_dir);

ReturnStr get_resource(const struct CContext *this_, const char *url, bool disable_cache);

ReturnStr get_app_info(const struct CContext *this_, const char *server, const char *id);
