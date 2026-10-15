
#include <sys/types.h>
#include <string.h>
#include <stdio.h>

#if defined(_MSC_VER)
#include <BaseTsd.h>
typedef SSIZE_T ssize_t;
#endif

#include "readstat_sas_rle.h"

#define SAS_RLE_COMMAND_COPY64          0
#define SAS_RLE_COMMAND_COPY64_PLUS_4096 1
#define SAS_RLE_COMMAND_COPY96          2
#define SAS_RLE_COMMAND_INSERT_BYTE18   4
#define SAS_RLE_COMMAND_INSERT_AT17     5
#define SAS_RLE_COMMAND_INSERT_BLANK17  6
#define SAS_RLE_COMMAND_INSERT_ZERO17   7
#define SAS_RLE_COMMAND_COPY1           8
#define SAS_RLE_COMMAND_COPY17          9
#define SAS_RLE_COMMAND_COPY33         10
#define SAS_RLE_COMMAND_COPY49         11
#define SAS_RLE_COMMAND_INSERT_BYTE3   12
#define SAS_RLE_COMMAND_INSERT_AT2     13
#define SAS_RLE_COMMAND_INSERT_BLANK2  14
#define SAS_RLE_COMMAND_INSERT_ZERO2   15

#define MAX_INSERT_RUN 4112 // 4095 + 17
#define MAX_COPY_RUN 4159 // 4095 + 64

static size_t command_lengths[16] = {
    [SAS_RLE_COMMAND_COPY64] = 1,
    [SAS_RLE_COMMAND_COPY64_PLUS_4096] = 1,
    [SAS_RLE_COMMAND_INSERT_BYTE18] = 2,
    [SAS_RLE_COMMAND_INSERT_AT17] = 1,
    [SAS_RLE_COMMAND_INSERT_BLANK17] = 1,
    [SAS_RLE_COMMAND_INSERT_ZERO17] = 1,
    [SAS_RLE_COMMAND_INSERT_BYTE3] = 1
};

ssize_t sas_rle_decompressed_len(const void *input_buf, size_t input_len) {
    return sas_rle_decompress(NULL, 0, input_buf, input_len);
}

ssize_t sas_rle_decompress(void *output_buf, size_t output_len, 
        const void *input_buf, size_t input_len) {
    unsigned char *buffer = (unsigned char *)output_buf;
    unsigned char *output = buffer;
    size_t output_written = 0;

    const unsigned char *input = (const unsigned char *)input_buf;

    while (input < (const unsigned char *)input_buf + input_len) {
        unsigned char control = *input++;
        unsigned char command = (control & 0xF0) >> 4;
        unsigned char length = (control & 0x0F);
        int copy_len = 0;
        int insert_len = 0;
        unsigned char insert_byte = '\0';
        if (input + command_lengths[command] > (const unsigned char *)input_buf + input_len) {
            return -1;
        }
        switch (command) {
            case SAS_RLE_COMMAND_COPY64:
                copy_len = (*input++) + 64 + length * 256;
                break;
            case SAS_RLE_COMMAND_COPY64_PLUS_4096:
                copy_len = (*input++) + 64 + length * 256 + 4096;
                break;
            case SAS_RLE_COMMAND_COPY96: copy_len = length + 96; break;
            case SAS_RLE_COMMAND_INSERT_BYTE18:
                insert_len = (*input++) + 18 + length * 256;
                insert_byte = *input++;
                break;
            case SAS_RLE_COMMAND_INSERT_AT17:
                insert_len = (*input++) + 17 + length * 256;
                insert_byte = '@';
                break;
            case SAS_RLE_COMMAND_INSERT_BLANK17:
                insert_len = (*input++) + 17 + length * 256;
                insert_byte = ' ';
                break;
            case SAS_RLE_COMMAND_INSERT_ZERO17:
                insert_len = (*input++) + 17 + length * 256;
                insert_byte = '\0';
                break;
            case SAS_RLE_COMMAND_COPY1:  copy_len = length + 1; break;
            case SAS_RLE_COMMAND_COPY17: copy_len = length + 17; break;
            case SAS_RLE_COMMAND_COPY33: copy_len = length + 33; break;
            case SAS_RLE_COMMAND_COPY49: copy_len = length + 49; break;
            case SAS_RLE_COMMAND_INSERT_BYTE3:
                insert_byte = *input++;
                insert_len = length + 3;
                break;
            case SAS_RLE_COMMAND_INSERT_AT2:
                insert_byte = '@';
                insert_len = length + 2;
                break;
            case SAS_RLE_COMMAND_INSERT_BLANK2:
                insert_byte = ' ';
                insert_len = length + 2;
                break;
            case SAS_RLE_COMMAND_INSERT_ZERO2:
                insert_byte = '\0';
                insert_len = length + 2;
                break;
            default:
                /* error out here? */
                break;
        }
        if (copy_len) {
            if (output_written + copy_len > output_len) {
                return -1;
            }
            if (input + copy_len > (const unsigned char *)input_buf + input_len) {
                return -1;
            }
            if (output) {
                memcpy(&output[output_written], input, copy_len);
            }
            input += copy_len;
            output_written += copy_len;
        }
        if (insert_len) {
            if (output_written + insert_len > output_len) {
                return -1;
            }
            if (output) {
                memset(&output[output_written], insert_byte, insert_len);
            }
            output_written += insert_len;
        }
    }

    return output_written;
}

/* yamaa read-only subset omits RLE compression/writer helpers. */
