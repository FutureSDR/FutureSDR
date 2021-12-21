var gulp = require('gulp');
var sass = require('gulp-sass');
var sourcemaps   = require('gulp-sourcemaps');
var cssmin = require('gulp-cssmin');
var postcss = require('gulp-postcss');
var debug = require('gulp-debug');
var autoprefixer = require('autoprefixer');
var browserSync = require('browser-sync');
var reload = browserSync.reload;

gulp.task('assets:css', function() {
    return gulp.src('assets/css/futuresdr.scss')
        .pipe(sourcemaps.init())
        .pipe(sass())
        .pipe(postcss([autoprefixer()]))
        .pipe(cssmin())
        .pipe(sourcemaps.write('.'))
        .pipe(gulp.dest('dist/css'))
        .pipe(browserSync.stream());
});

gulp.task('assets:static', function() {
    return gulp.src('assets/static/**/*')
        .pipe(gulp.dest('dist/'))
        .pipe(browserSync.stream());
});

gulp.task('assets', gulp.parallel('assets:css', 'assets:static'));
gulp.task('default', gulp.parallel('assets'));

gulp.task('serve', function() {

    gulp.watch('assets/css/**/*', gulp.task('assets:css'));
    gulp.watch('assets/static/**/*', gulp.task('assets:static'));

    browserSync({
        server: './dist',
        open: false
    });
});
