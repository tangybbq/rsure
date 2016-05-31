This directory is an 'rsure' BitKeeper store.  Stored within the BitKeeper
data are surefiles that represent the state of one or more filesystems at
one or more points in time.  You can use BitKeeper to see what is here.

  bk changes -v

will show you the revisions.  You can verify a revision manually with
something like

  bk co -r1.8 -p filename.dat | gzip > /tmp/filename.dat.gz
  rsure check -d dirname -f /tmp/filename.dat.gz
