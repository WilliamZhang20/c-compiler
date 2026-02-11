#ifndef _STDIO_H
#define _STDIO_H

typedef struct FILE FILE;

extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

int printf(const char *format, ...);
int fprintf(FILE *stream, const char *format, ...);
int sprintf(char *str, const char *format, ...);
int snprintf(char *str, unsigned long size, const char *format, ...);

int scanf(const char *format, ...);
int fscanf(FILE *stream, const char *format, ...);
int sscanf(const char *str, const char *format, ...);

FILE *fopen(const char *filename, const char *mode);
int fclose(FILE *stream);
int fflush(FILE *stream);

int fgetc(FILE *stream);
char *fgets(char *str, int n, FILE *stream);
int fputc(int char, FILE *stream);
int fputs(const char *str, FILE *stream);

int getc(FILE *stream);
int getchar(void);
int putc(int char, FILE *stream);
int putchar(int char);
int puts(const char *str);

void perror(const char *s);

#endif
