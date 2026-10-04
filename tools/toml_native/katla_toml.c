/* Byte-only TOML parsing and scalar queries; callers retain the opaque tree. */
#include "vendor/tomlc17.h"
#include <stdlib.h>
#include <string.h>

void *katla_toml_parse(const char *data, int length) {
    if (!data || length < 0 || length > 1024 * 1024) return NULL;
    toml_result_t *result = malloc(sizeof(*result));
    if (!result) return NULL;
    *result = toml_parse(data, length);
    if (!result->ok) { toml_free(*result); free(result); return NULL; }
    return result;
}
void katla_toml_destroy(void *tree) {
    if (!tree) return;
    toml_result_t *result = tree;
    toml_free(*result); free(result);
}
/* 0 missing, 1 string, 2 number, 3 bool, 4 table, -1 other valid TOML type. */
int katla_toml_find(void *tree, const char *path, const char **text, int *length, double *number) {
    if (!tree || !path || !text || !length || !number) return -1;
    toml_result_t *result = tree;
    toml_datum_t value = toml_seek(result->toptab, path);
    *text = NULL; *length = 0; *number = 0;
    switch (value.type) {
    case TOML_UNKNOWN: return 0;
    case TOML_STRING: *text = value.u.str.ptr; *length = value.u.str.len; return 1;
    case TOML_INT64: *number = (double)value.u.int64; return 2;
    case TOML_FP64: *number = value.u.fp64; return 2;
    case TOML_BOOLEAN: *number = value.u.boolean ? 1 : 0; return 3;
    case TOML_TABLE: return 4;
    default: return -1;
    }
}
