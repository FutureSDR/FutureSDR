var gulp = require('gulp');
var sass = require('gulp-sass');
var browserSync = require('browser-sync');
var reload = browserSync.reload;

gulp.task('assets:css', function() {
    return gulp.src('assets/css/futuresdr.scss')
        .pipe(sass())
        .pipe(gulp.dest('dist/css'))
        .pipe(browserSync.stream());
});

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

gulp.task('assets', gulp.parallel('assets:css', 'assets:static', 'assets:futuresdr'));
gulp.task('default', gulp.parallel('assets'));

gulp.task('serve', function() {

    gulp.watch('assets/css/**/*', gulp.task('assets:css'));
    gulp.watch('assets/static/**/*', gulp.task('assets:static'));

    browserSync({
        server: './dist',
        open: false
    });
});
