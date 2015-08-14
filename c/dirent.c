/* Dirent binding. */

#include <stddef.h>
#include <dirent.h>
#include <stdio.h>

/* The C directory entry API is utterly insane.  To avoid all of the
 * silliness involved, use a callback to pass the names back. */

size_t dir_name_offset(void) {
	return offsetof(struct dirent, d_name);
}

void blop(void) {
	printf("size: %d\n", sizeof(struct dirent));
}

int scan_dir(const char *path) {
	DIR *dp;

	dp = opendir(path);
	if (dp == NULL) {
		return -1;
	}

	while (1) {
	}

	closedir(dp);
	return 0;
}
