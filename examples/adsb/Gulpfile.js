var gulp = require('gulp');
var sass = require('gulp-sass')(require('sass'));
var browserSync = require('browser-sync');
var reload = browserSync.reload;

gulp.task('assets:static', function() {
    return gulp.src('assets/static/**/*')
        .pipe(gulp.dest('dist/'))
        .pipe(browserSync.stream());
});

gulp.task('assets:futuresdr', function() {
    return gulp.src('../../frontend/dist/futuresdr*')
        .pipe(gulp.dest('dist/'))
        .pipe(browserSync.stream());
});

gulp.task('assets', gulp.parallel('assets:static', 'assets:futuresdr'));
gulp.task('default', gulp.parallel('assets'));

gulp.task('serve', function() {

    gulp.watch('assets/static/**/*', gulp.task('assets:static'));

    browserSync({
        server: './dist',
        open: false
    });
});
