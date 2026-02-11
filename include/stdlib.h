#ifndef _STDLIB_H
#define _STDLIB_H

typedef unsigned long size_t;

void *malloc(size_t size);
void *calloc(size_t nitems, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);

void exit(int status);
void abort(void);

int system(const char *command);

int atoi(const char *str);
long atol(const char *str);
double atof(const char *str);

int rand(void);
void srand(unsigned int seed);

#endif
