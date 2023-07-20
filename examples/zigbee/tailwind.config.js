module.exports = {
  mode: "jit",
  content: {
    files: ["src/**/*.rs", "index.html"],
  },
  theme: {
    extend: {
      colors: {
        'fs-gray': '#dddddd',
        'fs-blue': 'rgb(62, 113, 145)',
        'fs-green': '#589068',
        'fs-darkgrayp': '#545454',
      },
    },
  },
  variants: {
    extend: {},
  },
  plugins: [],
};
