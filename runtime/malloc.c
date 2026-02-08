#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#define MAGIC 0x534146454D414C43ULL // "SAFEMALC"
#define POISON 0xDE
#define CANARY 0xABCD1234
#define CANARY_SIZE 8

typedef struct {
    uint64_t magic;
    size_t size;
    uint32_t checksum;
    uint32_t padding;
} BlockHeader;

static uint32_t calculate_checksum(BlockHeader* header) {
    uint32_t checksum = 0;
    checksum ^= (uint32_t)header->magic;
    checksum ^= (uint32_t)(header->magic >> 32);
    checksum ^= (uint32_t)header->size;
#if defined(_WIN64)
    checksum ^= (uint32_t)(header->size >> 32);
#endif
    return checksum;
}

void* safe_malloc(size_t size) {
    size_t total_size = sizeof(BlockHeader) + CANARY_SIZE + size + CANARY_SIZE;
    void* raw = VirtualAlloc(NULL, total_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!raw) return NULL;

    BlockHeader* header = (BlockHeader*)raw;
    header->magic = MAGIC;
    header->size = size;
    header->checksum = calculate_checksum(header);

    uint8_t* payload = (uint8_t*)raw + sizeof(BlockHeader) + CANARY_SIZE;
    
    // Set canaries
    uint32_t* pre_canary = (uint32_t*)((uint8_t*)payload - CANARY_SIZE);
    uint32_t* post_canary = (uint32_t*)(payload + size);
    *pre_canary = CANARY;
    *post_canary = CANARY;

    return payload;
}

void safe_free(void* ptr) {
    if (!ptr) return;

    BlockHeader* header = (BlockHeader*)((uint8_t*)ptr - CANARY_SIZE - sizeof(BlockHeader));
    
    // 1. Check Magic
    if (header->magic != MAGIC) {
        fprintf(stderr, "FATAL: Memory corruption detected (Invalid Magic)!\n");
        exit(1);
    }

    // 2. Check Checksum
    if (header->checksum != calculate_checksum(header)) {
        fprintf(stderr, "FATAL: Memory corruption detected (Header Tampered)!\n");
        exit(1);
    }

    size_t size = header->size;

    // 3. Check Canaries
    uint32_t* pre_canary = (uint32_t*)((uint8_t*)ptr - CANARY_SIZE);
    uint32_t* post_canary = (uint32_t*)((uint8_t*)ptr + size);
    if (*pre_canary != CANARY || *post_canary != CANARY) {
        fprintf(stderr, "FATAL: Buffer overflow detected (Canary corrupted)!\n");
        exit(1);
    }

    // 4. Poisoning
    memset(ptr, POISON, size);

    // 5. Actually Free (or keep around to catch UAF? For now just free)
    // In a "real" safe allocator we might use VirtualFree to mark the page inaccessible
    // But since we allocate smaller blocks within pages usually, let's just free the whole allocation for now.
    VirtualFree(header, 0, MEM_RELEASE);
}

// Map malloc/free to our safe versions for the compiler to use
void* malloc(size_t size) { return safe_malloc(size); }
void free(void* ptr) { safe_free(ptr); }
