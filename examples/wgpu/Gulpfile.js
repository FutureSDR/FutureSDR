var gulp = require('gulp');
var browserSync = require('browser-sync');
var reload = browserSync.reload;

gulp.task('assets:static', function() {
    return gulp.src('assets/static/**/*')
        .pipe(gulp.dest('dist/'))
        .pipe(browserSync.stream());
});

gulp.task('default', gulp.task('assets:static'));

gulp.task('serve', function() {

    gulp.watch('assets/static/**/*', gulp.task('assets:static'));

    browserSync({
        server: './dist',
        open: false
    });
});
