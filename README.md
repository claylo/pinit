# pinit

`pinit` is a new project "generator" for lazy-yet-detail-oriented people like myself.


## Prior Art

It's not like I'm the first to say, "hey, starting a new project should suck less!" So why `newbie` and not just one of the other approaches? Well...

### What about Yeoman?

I've always wanted to love [Yeoman](https://yeoman.io), and it's an excellent fit for many people. It's very JavaScripty, though, and I've had too many projects start with a failing `yo` generator. Not a great way to get started. Plus, many templates are close to what I might want but differ just enough to make the post-generation clean-up process annoying and not repeatable.

### Okay, `git init` templates!

"`git init` supports template directories," you say. "Why not use a series of those instead of creating a whole new thing?"

Read the [documentation on template directories](https://git-scm.com/docs/git-init#_template_directory), and you'll see the problem with `git init`. (Emphasis mine.)

> Files and directories in the template directory whose name **do not start with a dot** will be copied

How many *useful* template repositories would not contain file names beginning with a dot? :thinking:

### GitHub Repository Templates?

GitHub repository templates were [announced on June 6, 2019](https://github.blog/2019-06-06-generate-new-repositories-with-repository-templates/), and are [documented here](https://docs.github.com/en/repositories/creating-and-managing-repositories/creating-a-repository-from-a-template).

But:

* What if my new project isn't on GitHub?
* What if I don't know yet if I want a hosted remote?

### What if :scream: I'm not using git?

IKR? So crazy. :roll_eyes:

No, really: A whole bunch of folks use [Subversion](https://subversion.apache.org/). There's a thing called [Piper](https://cacm.acm.org/magazines/2016/7/204032-why-google-stores-billions-of-lines-of-code-in-a-single-repository/fulltext) that houses a gazillion lines of code. There's Perforce, Mercurial, and ... look at [this list](https://en.wikipedia.org/wiki/Comparison_of_version-control_software).



