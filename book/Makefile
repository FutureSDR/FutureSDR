.PHOHY: upload
upload:
	mdbook build
	rsync -az --progress -e ssh --delete book/ basti@fleark.de:/srv/www/futuresdr.org/learn/
