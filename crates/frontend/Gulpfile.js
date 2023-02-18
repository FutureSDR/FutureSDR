var gulp = require('gulp');
var sass = require('gulp-sass')(require('sass'));
var browserSync = require('browser-sync');
var reload = browserSync.reload;

gulp.task('assets:css', function() {
    return gulp.src('assets/css/futuresdr.scss')
        .pipe(sass())
        .pipe(gulp.dest('dist/css'))
        .pipe(browserSync.stream());
});

gulp.task('assets:js', function() {
    return gulp.src('assets/js/**/*')
        .pipe(gulp.dest('dist/js'))
        .pipe(browserSync.stream());
});

gulp.task('assets:static', function() {
    return gulp.src('assets/static/**/*')
        .pipe(gulp.dest('dist/'))
        .pipe(browserSync.stream());
});

gulp.task('assets', gulp.parallel('assets:css', 'assets:js', 'assets:static'));
gulp.task('default', gulp.parallel('assets'));

gulp.task('serve', function() {

    gulp.watch('assets/css/**/*', gulp.task('assets:css'));
    gulp.watch('assets/static/**/*', gulp.task('assets:static'));

    browserSync({
        server: './dist',
        open: false
    });
});

// gulp.task('serve', gulp.series('default')) 
