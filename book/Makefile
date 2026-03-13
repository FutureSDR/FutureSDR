.PHOHY: upload
upload:
	mdbook build
	rsync -az --progress -e ssh --delete book/ nginx:/srv/futuresdr.org/learn/
