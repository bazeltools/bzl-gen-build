package io.bazeltools.buildgen.scaladefref

import cats.Monad
import cats.data.{Chain, NonEmptyList, State, Writer}
import cats.effect.IO
import scala.collection.immutable.{SortedMap, SortedSet}
import scala.meta.inputs.Input
import scala.meta.parsers.Parsed
import scala.meta.{
  Import,
  Importer,
  Importee,
  Name,
  Lit,
  Source,
  Term,
  Tree,
  Pat
}

import cats.syntax.all._
import io.bazeltools.buildgen.shared.{Entity, PathTree, DataBlock}

object ScalaSourceEntityExtractor {
  case class ParseException(parseError: Parsed.Error)
      extends Exception(s"parse error: $parseError")

  def parseDirectives(src: Source): Chain[String] = {
    import scala.meta.tokens.Token
    cats.Monoid.combineAll(src.tokens.collect { case Token.Comment(c) =>
      val str = c.toString
      Entity.findDirectives(str) match {
        case Right(ds) => ds
        case Left(err) =>
          sys.error(s"couldn't parse\n${str}\n-------------\n$err")
      }
    })
  }

  def extract(content: String): IO[DataBlock] =
    Input
      .VirtualFile("Source.scala", content)
      .parse[Source]
      .fold({ e => IO.raiseError(ParseException(e)) }, IO.pure(_))
      .map { tree =>
        val allDirectives = parseDirectives(tree)
        val dr = getDefsRefs(tree)
        allDirectives.foldLeft(dr) { case (prev, n) =>
          prev.addBzlBuildGenCommand(n)
        }
      }

  sealed abstract class NamePart {
    def toEntity: Option[Entity]
  }
  object NamePart {
    case class Package(entity: Entity) extends NamePart {
      def toEntity = Some(entity)
    }
    case class Defn(entity: Entity) extends NamePart {
      def toEntity = Some(entity)
    }
    case object Anonymous extends NamePart {
      def toEntity = None
    }

    /** a list of all packages a scope with a vector of NameParts a non-local
      * reference could refer to
      */
    def referencePackages(path: Seq[NamePart]): NonEmptyList[Entity] = {
      def loop(pathList: List[NamePart]): List[Entity] =
        pathList match {
          case NamePart.Package(ent) :: tail =>
            // package foo {
            //   package bar {
            //     // refer to name x could be x, foo.x, foo.bar.x
            //   }
            // }
            ent :: loop(tail).map(ent / _)
          case _ => Nil
        }

      NonEmptyList(Entity.empty, loop(path.toList))
    }
  }

  // TODO: we have to track terms and types separately. We can't forget which ones have been defined
  // since we can have shadowing of one type that might look like another
  case class ScopeState(
      key: List[Long],
      path: Vector[NamePart],
      imports: Vector[Import],
      defs: Vector[Name],
      refs: Vector[NonEmptyList[Name]],
      links: Vector[(Name, NonEmptyList[Name])]
  ) {
    def addRef(n: NonEmptyList[Name]): ScopeState = copy(refs = refs :+ n)
    def addLink(src: Name, targets: NonEmptyList[Name]): ScopeState =
      copy(links = links :+ (src, targets))
    def addDef(n: Name): ScopeState = copy(defs = defs :+ n)

    def addImport(i: Import): ScopeState = copy(imports = imports :+ i)

    def entity: Option[Entity] =
      if (path.isEmpty) None
      else {
        path.iterator.map(_.toEntity).reduce { (a, b) => (a, b).mapN(_ / _) }
      }

    def localEntity: Entity.Resolved =
      Entity.Resolved.Local(key, Entity.empty)

    def definedEntities: SortedSet[Entity] =
      entity match {
        case None => SortedSet.empty[Entity]
        case Some(n) =>
          defs.iterator.map { name => n / name.value }.to(SortedSet)
      }

    def linkSources: List[(Entity, NonEmptyList[Name])] =
      entity match {
        case None => Nil
        case Some(n) =>
          links.iterator.map { case (name, targets) =>
            (n / name.value, targets)
          }.toList
      }

    def collectImports[A](
        fn: PartialFunction[(Entity, Importee), A]
    ): Iterator[A] =
      for {
        importStmt <- imports.iterator
        Importer(ref, importees) <- importStmt.importers.iterator
        entity = termToEntity(ref)
        imp <- importees.filter { i => fn.isDefinedAt((entity, i)) }
      } yield fn((entity, imp))

    def unresolvedWildcards: List[Entity] =
      collectImports { case (e, Importee.Wildcard()) =>
        e
      }
        .to(List)

    def unresolvedUnimports: SortedSet[Entity] =
      collectImports { case (e, Importee.Unimport(_)) =>
        e
      }
        .to(SortedSet)

    def unresolvedImportRefs: SortedMap[String, Entity] =
      collectImports {
        case (entity, Importee.Name(n)) =>
          val str = n.value
          (str, entity / str)
        case (entity, Importee.Rename(from, to)) =>
          (to.value, entity / from.value)
      }
        .to(SortedMap)

    def allUnresolvedImportEntities: Iterator[Entity] =
      unresolvedUnimports.iterator ++ unresolvedWildcards.iterator ++
        unresolvedImportRefs.iterator.map(_._2)

    // We can ignore local defs because they are always in the current
    // compilation unit
    def nonLocalRefs(resolve: String => Entity.Resolved): SortedSet[Entity] = {
      def resolveName[A](
          nel: NonEmptyList[A]
      )(fn: A => String): Entity.Resolved = {
        val root = resolve(fn(nel.head))
        nel.tail.foldLeft(root) { (r, p) => r / fn(p) }
      }

      def resolveNonLocals[A](
          as: Iterable[NonEmptyList[A]]
      )(valueOf: A => String): SortedSet[Entity] =
        as.iterator
          .map(resolveName(_)(valueOf))
          .flatMap(_.entities.toList)
          .to(SortedSet)

      val directRefs = resolveNonLocals(refs)(_.value)
      val imps = resolveNonLocals(
        allUnresolvedImportEntities
          .flatMap(_.prefixes.toList)
          .collect {
            case Entity(v) if v.nonEmpty =>
              NonEmptyList.fromListUnsafe(v.toList)
          }
          .toList
      )(identity(_))

      directRefs | imps
    }

    def packageResolved: Entity.Resolved =
      NamePart
        .referencePackages(path)
        .map(Entity.Resolved.Known(_): Entity.Resolved)
        .reduceLeft(Entity.Resolved.Many(_, _))
  }

  object ScopeState {
    def empty(key: List[Long], path: Vector[NamePart]): ScopeState =
      ScopeState(
        key,
        path,
        Vector.empty,
        Vector.empty,
        Vector.empty,
        Vector.empty
      )
  }

  case class ScopeTree[+V](
      parent: Option[ScopeTree[V]],
      next: Long,
      current: List[Long],
      path: Vector[NamePart],
      tree: PathTree[Long, V]
  ) {
    def emptyScope: ScopeState = ScopeState.empty(current, path)

    // called on the parent and return a new child
    def startChild(name: NamePart): ScopeTree[V] = {
      val updatedParent = copy(next = next + 1L)
      ScopeTree(
        Some(updatedParent),
        0L,
        current :+ next,
        path :+ name,
        PathTree.empty
      )
    }

    // called on the child and return the updated parent
    def endChild: ScopeTree[V] =
      parent match {
        case None =>
          throw new IllegalStateException(s"parentless child: $this")
        case Some(p) =>
          // merge our tree into our parents at the right path
          val tree1 = p.tree.updateSubTree(current, tree.getSubTree(current))
          p.copy(tree = tree1)
      }
  }

  object ScopeTree {
    val empty: ScopeTree[Nothing] =
      ScopeTree(None, 0L, Nil, Vector.empty, PathTree.empty[Long])
  }

  type SS = ScopeTree[ScopeState]
  type Env[A] = State[SS, A]

  def updateState(fn: ScopeState => ScopeState): Env[Unit] =
    State { st =>
      val t1 = st.tree.transform(st.current) {
        case Some(ss) => Some(fn(ss))
        case None     => Some(fn(st.emptyScope))
      }

      (st.copy(tree = t1), ())
    }

  def define(n: Name): Env[Unit] =
    updateState(_.addDef(n))

  def link(n: Name, target: List[Name]): Env[Unit] = {
    if (target.nonEmpty) {
      updateState(_.addLink(n, NonEmptyList.fromListUnsafe(target)))
    } else {
      State.empty
    }
  }

  def link(n: Name, target: Name): Env[Unit] =
    link(n, target :: Nil)

  // val Foo(bar, baz) = ...
  // so we need to define all the names we hit
  def definePat(n: Pat): Env[Unit] =
    n match {
      case Pat.Alternative(_, _) =>
        // scala doesn't allow you to do use alternation in binds
        unitEnv
      case Pat.Bind(left, pat) =>
        definePat(left) *> definePat(pat)
      case Pat.Interpolate(t, _, args) =>
        referTo(t) *> args.traverse_(definePat)
      case Pat.Var(name) =>
        define(name)
      case Pat.Tuple(items) =>
        items.traverse_(definePat)
      case Pat.Typed(pat, tpe) =>
        inspect(tpe) *> definePat(pat)
      case Pat.Wildcard() => unitEnv
      case Pat.Extract(term, args) =>
        inspect(term) *> args.traverse_(definePat)
      case Pat.ExtractInfix(term, op, args) =>
        referTo(op) *> (term :: args).traverse_(definePat)
      case Pat.SeqWildcard() => unitEnv
      case _: Lit            => unitEnv
      case tn: Term.Name     => referTo(tn)
      case other =>
        sys.error(s"unexpected: $other, ${other.getClass}")
    }

  def referTo(n: Name): Env[Unit] =
    referTo(NonEmptyList(n, Nil))

  def referTo(n: NonEmptyList[Name]): Env[Unit] =
    updateState(_.addRef(n))

  // t.n
  def referSelected(t: Term, n: Term.Name): Env[Unit] =
    termToNames(Term.Select(t, n)) match {
      case Right(nel) => referTo(nel)
      case Left(_)    => Monad[Env].unit
    }

  def scope[A](namePart: NamePart, env: Env[A]): Env[A] = {
    for {
      current <- State.get: Env[SS]
      child = current.startChild(namePart)
      _ <- State.set(child): Env[Unit]
      a <- env
      childDone <- State.get: Env[SS]
      ended = childDone.endChild
      _ <- State.set(ended): Env[Unit]
    } yield a
  }

  // a named scope (but no define)
  def inside[A](t: Name, env: Env[A]): Env[A] =
    scope(NamePart.Defn(Entity.simple(t.value)), env)

  // we expect a select chain of names
  def termToNames(t: Term): Either[Term, NonEmptyList[Name]] = {
    @annotation.tailrec
    def loop(t: Term, acc: List[Name]): Either[Term, NonEmptyList[Name]] =
      t match {
        case n @ Term.Name(_) => Right(NonEmptyList(n, acc))
        case Term.Select(left, n @ Term.Name(_)) =>
          loop(left, n :: acc)
        case other => Left(other)
      }

    loop(t, Nil)
  }

  private def termToEntity(t: Term): Entity =
    termToNames(t) match {
      case Right(ns) =>
        Entity(ns.toList.iterator.map(_.value).toVector)
      case Left(other) => sys.error(s"unexpected: ${other.getClass}, $other")
    }

  def insidePackage[A](t: Term, env: Env[A]): Env[A] =
    scope(NamePart.Package(termToEntity(t)), env)

  // introduced by blocks
  def newScope[A](env: Env[A]): Env[A] =
    scope(NamePart.Anonymous, env)

  def addImport(im: Import): Env[Unit] =
    updateState(_.addImport(im))

  val unitEnv: Env[Unit] = Monad[Env].unit

  def processScopeTree(pt: PathTree[Long, ScopeState]): DataBlock = {

    // For ScopeState in this pathTree we can use reference equality for
    // caching
    def memoize[A, B](fn: ScopeState => A => B): ScopeState => A => B = {
      val outerMap = new java.util.IdentityHashMap[ScopeState, A => B]()

      { (ss: ScopeState) =>
        outerMap.get(ss) match {
          case null =>
            val innerMap = new java.util.HashMap[A, B]()
            val innerFn = fn(ss)

            { (a: A) =>
              innerMap.get(a) match {
                case null =>
                  val b = innerFn(a)
                  innerMap.put(a, b)
                  b
                case b => b
              }
            }
          case fn => fn
        }
      }
    }

    def getOuter(ss: ScopeState): Option[ScopeState] = {
      @annotation.tailrec
      def loop(k: List[Long]): Option[ScopeState] =
        k match {
          case Nil => None
          case notNil =>
            val kParent = notNil.init
            pt.getAt(kParent) match {
              case Some(ss) => Some(ss)
              case None     => loop(kParent)
            }
        }

      loop(ss.key)
    }

    def innerToOuter(ss: ScopeState): NonEmptyList[ScopeState] =
      getOuter(ss) match {
        case None       => NonEmptyList(ss, Nil)
        case Some(prev) => ss :: innerToOuter(prev)
      }

    lazy val getResolvesDefinite
        : ScopeState => String => Option[Entity.Resolved] =
      memoize { (ss: ScopeState) =>
        val parentFn: Option[String => Option[Entity.Resolved]] =
          getOuter(ss).map(getResolvesDefinite(_))
        val localDefs = ss.defs.iterator.map(_.value).to(SortedSet)
        val impRefs = ss.unresolvedImportRefs
        val locE = ss.localEntity
        lazy val wildRes = getResolveWild(ss)

        { (name: String) =>
          if (localDefs(name)) {
            Some(locE / name)
          } else
            impRefs.get(name) match {
              case Some(e) =>
                // We have to resolve the import
                Some(e.resolve(wildRes))
              case None =>
                // it is not a local name, or imported locally, try the parent
                parentFn.flatMap(_(name))
            }
        }
      }

    @annotation.tailrec
    def getLinksRecursive(
        ss: ScopeState,
        acc: List[(Entity, NonEmptyList[Name])]
    ): List[(Entity, NonEmptyList[Name])] = {
      val nxt = ss.linkSources ::: acc
      getOuter(ss) match {
        case None    => nxt
        case Some(p) => getLinksRecursive(p, nxt)
      }
    }

    /*
     * Try to do a definite resolve, otherwise fall back to wildcard imports
     * NOT package scope
     */
    lazy val getResolveWild: ScopeState => String => Entity.Resolved =
      memoize { (ss: ScopeState) =>
        val parentResolver: Option[String => Entity.Resolved] =
          getOuter(ss).map(getResolve(_))

        val initRes = parentResolver.getOrElse { (str: String) =>
          Entity.Resolved.Known(Entity.simple(str))
        }

        val defRes = getResolvesDefinite(ss)

        val thisUWild = ss.unresolvedWildcards

        // resolve each item using all outer and previous scopes
        lazy val thisRWild: String => Entity.Resolved =
          thisUWild.foldLeft(initRes) { (acc, uwild) =>
            val rWild = uwild.resolve(acc)

            { (name: String) => acc(name) | (rWild / name) }
          }

        { (name: String) =>
          defRes(name) match {
            case Some(r) => r
            case None    =>
              // If we don't know the name, it could be in the package scope, or any in scope wildcards
              // if we have
              // import foo._
              // import bar._
              // then bar needs to be imported against all it's previous items
              thisRWild(name)
          }
        }
      }

    // If not definite, then wild | package
    lazy val getResolve: ScopeState => String => Entity.Resolved =
      memoize { (ss: ScopeState) =>
        val definite = getResolvesDefinite(ss)
        val wild = getResolveWild(ss)
        val packRes = ss.packageResolved

        { (name: String) =>
          definite(name) match {
            case Some(e) => e
            case None =>
              wild(name) | (packRes / name)
          }
        }
      }

    def processScope(s: ScopeState): Writer[DataBlock, Unit] = {
      val defs = s.definedEntities
      val refs = s.nonLocalRefs(getResolve(s))
      val stringRefs = refs.map(_.toString)
      val links = getLinksRecursive(s, Nil).iterator
        .flatMap { case (src, dests) =>
          dests.iterator.iterator
            .map(_.value)
            .flatMap { e =>
              val data = e.split('.').toList
              val h :: tail = data
              stringRefs.iterator.filter { y => y.endsWith(h) }.flatMap { e =>
                e :: tail.map { t =>
                  s"${e}.$t"
                }
              }
            }
            .map { linkTarget =>
              s"link: ${src} -> $linkTarget"
            }
        }
        .to(SortedSet)
      Writer.tell(
        DataBlock("", defs = defs, refs = refs, bzl_gen_build_commands = links)
      )
    }

    pt.traverse(processScope).run._1
  }

  def getDefsRefs(tree: Tree): DataBlock = {
    val ss = inspect(tree).run(ScopeTree.empty: SS).value._1
    processScopeTree(ss.tree)
  }

  def log(s: String): Unit = {
    require(s ne null)
    // println(s)
  }

  def treeToNames(tpe: scala.meta.Tree): List[Name] = {
    import scala.meta._
    tpe match {
      case Defn.Def(mods, name, tparams, paramss, optType, body) =>
        log(
          s"treeToNames:  Defn.Def(mods:$mods, name: $name, tparams: $tparams, paramss: $paramss, optType: $optType, body: $body)"
        )
        optType.toList.map(typeToNames).flatten ++
          paramss.flatMap(_.map(treeToNames)).flatten
      case Term.Param(mods, name, optType, optTerm) =>
        log(s"treeToNames: Term.Param($mods, $name, $optType, $optTerm)")
        optType.toList.map(typeToNames).flatten
      case Init(tpe, name, terms) =>
        log(s"Init($tpe, $name, $terms)")
        typeToNames(tpe)

      case Defn.Val(mods, pats, optType, term) =>
        log(
          s"treeToNames: Val(mods: $mods, pats: $pats, optType: $optType, term: $term)"
        )
        optType.toList.flatMap(typeToNames)
      case _ => Nil

      // case t =>
      // sys.error(s"unknown: ${t.getClass} $t")
    }
  }
  def typeToNames(tpe: scala.meta.Type): List[Name] = {
    import scala.meta._
    tpe match {
      case tn @ Type.Name(_) =>
        List(tn)
      case Type.Apply(left, args) =>
        log(s"typeToNames: Type.Apply($left, $args)")
        (left :: args).traverse(typeToNames).flatten
      case s @ Type.Select(_, _) =>
        List(Name(s.toString))
      case _ => Nil
      // case t =>
      // sys.error(s"unknown: ${t.getClass} $t")
    }
  }

  /** Recurse all the way through Tree building up a scope map once we have done
    * this pass building the full scope map, we process each scope to get the
    * defs and refs
    */
  def inspect(tree: Tree): Env[Unit] = {
    import scala.meta._

    tree match {
      case Case(pat, cond, body) =>
        // case pat if cond => body
        log(s"Case($pat, $cond, $body)")
        newScope(
          List(
            inspect(pat),
            cond.traverse_(inspect),
            inspect(body)
          ).sequence_
        )
      case Decl.Def(mods, name, tparams, paramss, decltpe) =>
        log(s"Decl.Def($mods, $name, $tparams, $paramss, $decltpe)")
        List(
          define(name),
          tparams.traverse_(inspect),
          paramss.traverse_(_.traverse_(inspect)),
          inspect(decltpe)
        ).sequence_
      case Decl.Val(mods, pats, decltpe) =>
        log(s"Decl.Val($mods, $pats, $decltpe)")
        List(
          pats.traverse_(inspect),
          inspect(decltpe)
        ).sequence_
      case Decl.Type(mods, name, tparams, bounds) =>
        log(s"Decl.Type($mods, $name, $tparams, $bounds)")
        List(
          define(name),
          tparams.traverse_(inspect),
          inspect(bounds)
        ).sequence_
      case Enumerator.Generator(left, right) =>
        // a <- f
        log(s"Enumerator.Generator($left, $right)")
        List(left, right).traverse_(inspect)
      case Enumerator.Guard(term) =>
        // if foo
        log(s"Enumerator.Guard($term)")
        inspect(term)
      case Enumerator.Val(left, right) =>
        // in for
        // a = f
        log(s"Enumerator.Val($left, $right)")
        List(left, right).traverse_(inspect)
      case Type.ByName(n) =>
        // => foo
        log(s"Type.ByName($n)")
        inspect(n)
      case tn @ Type.Name(n) =>
        log(s"type name: $n")
        referTo(tn)
      case Type.Select(left, item) =>
        // foo.Bar
        log(s"Type.Select($left, $item)")
        List(left, item).traverse_(inspect)
      case Type.Annotate(tpe, annots) =>
        log(s"Type.Annotate($tpe, $annots)")
        List(inspect(tpe), annots.traverse_(inspect)).sequence_
      case Type.Apply(left, args) =>
        log(s"Type.Apply($left, $args)")
        (left :: args).traverse_(inspect)
      case Type.ApplyInfix(left, op, right) =>
        log(s"Type.ApplyInfix($left, $op, $right)")
        (left :: op :: right :: Nil).traverse_(inspect)
      case Type.Bounds(optLow, optHigh) =>
        log(s"Type.Bounds($optLow, $optHigh)")
        (optLow.toList ::: optHigh.toList).traverse_(inspect)
      case Type.Existential(tpe, stats) =>
        log(s"Type.Existential($tpe, $stats)")
        (tpe :: stats).traverse_(inspect)
      case Type.Function(params, res) =>
        // TODO params should be defined inside res
        log(s"Type.Function($params, $res)")
        (params ::: (res :: Nil)).traverse_(inspect)
      case Type.Param(mods, name, tparams, tbounds, vbounds, cbounds) =>
        // TODO this should be defining a name for the scope
        // it is set for
        log(s"Type.Param($mods, $name, $tparams, $tbounds, $vbounds, $cbounds)")
        List(
          tparams.traverse_(inspect),
          inspect(tbounds),
          vbounds.traverse_(inspect),
          cbounds.traverse_(inspect)
        ).sequence_
      case Type.Var(name) =>
        log(s"Type.Var($name)")
        unitEnv
      case Type.Project(tpe, name) =>
        // Foo#Bar
        log(s"Type.Project($tpe, $name)")
        // we just need to refer to tpe
        inspect(tpe)
      case Type.Refine(optType, stats) =>
        // Foo { type OutCol = Bar }
        log(s"Type.Refine($optType, $stats)")
        List(
          optType.traverse_(inspect),
          stats.traverse_(inspect)
        ).sequence_
      case Type.Repeated(tpe) =>
        log(s"Type.Repeated($tpe)")
        inspect(tpe)
      case Type.Tuple(ts) =>
        log(s"Type.Tuple($ts)")
        ts.traverse_(inspect)
      case Type.With(left, right) =>
        log(s"Type.With($left, $right)")
        List(left, right).traverse_(inspect)
      case Type.Placeholder(bounds) =>
        log(s"Type.Placeholder($bounds)")
        inspect(bounds)
      case Type.Singleton(n) =>
        // Foo.type
        log(s"Type.Singleton($n)")
        // just refer to the name above
        inspect(n)
      case Term.Do(body, cond) =>
        log(s"Term.Do($body, $cond)")
        List(inspect(body), inspect(cond)).sequence_
      case tn @ Term.Name(n) =>
        log(s"term name: $n")
        referTo(tn)
      case Term.Select(n, a) =>
        log(s"Term.Select($n, $a)")
        // we don't inspect a because it is a name
        // and we already refer to it in referSelected
        referSelected(n, a) *> inspect(n)
      case Term.Super(thisP, superP) =>
        log(s"Term.Super($thisP, $superP)")
        List(thisP, superP).traverse_(inspect)
      case Term.Param(mods, name, optType, optTerm) =>
        log(s"Term.Param($mods, $name, $optType, $optTerm)")
        (optType.toList ::: optTerm.toList).traverse_(inspect)
      case Term.AnonymousFunction(body) =>
        log(s"AnonymousFunction($body)")
        inspect(body)
      case Term.Annotate(term, annots) =>
        log(s"Term.Annotate($term, $annots)")
        (term :: annots).traverse_(inspect)
      case Term.ApplyInfix(term, name, typeArgs, termArgs) =>
        log(s"Term.ApplyInfix($term, $name, $typeArgs, $termArgs)")
        List(
          referSelected(term, name),
          inspect(term),
          typeArgs.traverse_(inspect),
          termArgs.traverse_(inspect)
        ).sequence_
      case Term.Apply(term, termArgs) =>
        log(s"Term.Apply($term, $termArgs)")
        (term :: termArgs).traverse_(inspect)
      case Term.ApplyUnary(op, term) =>
        log(s"Term.ApplyUnary($op, $term)")
        referSelected(term, op) *> inspect(op) *> inspect(term)
      case Term.ApplyType(tpe, tpeArgs) =>
        log(s"Term.ApplyType($tpe, $tpeArgs)")
        (tpe :: tpeArgs).traverse_(inspect)
      case Term.Ascribe(term, tpe) =>
        // r: tpe
        log(s"Term.Ascribe($term, $tpe)")
        List(term, tpe).traverse_(inspect)
      case Term.Assign(left, right) =>
        // left = right
        log(s"Term.Assign($left, $right)")
        List(left, right).traverse_(inspect)
      case Term.Block(items) =>
        log(s"Block($items)")
        newScope(items.traverse_(inspect))
      case Term.Eta(term) =>
        // fee _
        log(s"Term.Eta($term)")
        inspect(term)
      case Term.Match(arg, cases) =>
        log(s"Term.Match($arg, $cases)")
        (arg :: cases).traverse_(inspect)
      case Term.New(init) =>
        log(s"Term.New($init)")
        inspect(init)
      case Term.NewAnonymous(templ) =>
        log(s"Term.NewAnonymous($templ)")
        // do we need to remember if we are in a val/object or package?
        inspect(templ)
      case Term.PartialFunction(cases) =>
        log(s"Term.PartialFunction($cases)")
        cases.traverse_(inspect)
      case Term.Return(arg) =>
        log(s"Term.Return($arg)")
        inspect(arg)
      case Term.This(n) =>
        log(s"Term.This($n)")
        inspect(n)
      case Term.Throw(ex) =>
        log(s"Term.Throw($ex)")
        inspect(ex)
      case Term.Placeholder() =>
        log(s"Term.Placeholder")
        unitEnv
      case Term.Interpolate(name, parts, args) =>
        log(s"Term.Interpolate($name, $parts, $args)")
        List(referTo(name), args.traverse_(inspect)).sequence_
      case Term.If(cond, thenCase, elseCase) =>
        log(s"Term.If($cond, $thenCase, $elseCase")
        List(cond, thenCase, elseCase).traverse_(inspect)
      case Term.Function(params, body) =>
        log(s"Function($params, $body)")
        val defineParams = params.traverse_ {
          case Term.Param(_, name, optType, optTerm) =>
            optTerm.traverse_(inspect(_)) *>
              optType.traverse_(inspect(_)) *>
              define(name)
        }

        newScope(defineParams *> inspect(body))
      case Term.Repeated(r) =>
        log(s"Term.Repeated($r)")
        inspect(r)
      case Term.Try(expr, catches, fin) =>
        log(s"Term.Try($expr, $catches, $fin)")
        List(
          inspect(expr),
          catches.traverse_(inspect),
          fin.traverse_(inspect)
        ).sequence_
      case Term.Tuple(items) =>
        log(s"Term.Tuple($items)")
        items.traverse_(inspect)
      case Term.ForYield(enums, y) =>
        log(s"ForYield($enums, $y)")
        (enums ::: (y :: Nil)).traverse_(inspect)
      case Term.For(enums, y) =>
        log(s"For($enums, $y)")
        (enums ::: (y :: Nil)).traverse_(inspect)
      case Term.While(expr, body) =>
        log(s"Term.While($expr, $body)")
        List(expr, body).traverse_(inspect)
      case Source(stats) =>
        // can search comments here if needed
        log(s"source stats len=${stats.length}")
        stats.traverse_(inspect)
      case Pkg(name, items) =>
        log(s"pkg: $name, items size: ${items.size}")
        insidePackage(name, items.traverse_(inspect))
      case i @ Import(names) =>
        log(s"import: $names")
        addImport(i)
      case Init(tpe, name, terms) =>
        log(s"Init($tpe, $name, $terms)")
        // I guess we just traverse these things:
        (tpe :: name :: terms.flatten).traverse_(inspect)
      case nm @ Name(n) =>
        log(s"Name($n)")
        if (n.nonEmpty) referTo(nm)
        else unitEnv
      case Pkg.Object(mods, name, template @ Template(_, init, _, stats)) =>
        log(s"package object: mods = $mods, name = $name, template = $template")
        link(
          name,
          init.map(_.tpe).flatMap(typeToNames) ++ stats.flatMap(treeToNames)
        ) *> inside(name, inspect(template))
      case Template(early, init, self, stats) =>
        log(
          s"Template(early = $early, init = $init, self = $self, stats = $stats)"
        )
        (early ::: init ::: self :: stats).traverse_(inspect)
      case Defn.Class(
            mods,
            name,
            tparams,
            ctor,
            template @ Template(early, init, self, stats)
          ) =>
        log(
          s"Class($mods, $name, $tparams, $ctor, Template(early: $early, init: $init, self:$self, stats: $stats)"
        )

        define(name) *> link(
          name,
          init.map(_.tpe).flatMap(typeToNames) ++ stats.flatMap(treeToNames)
        ) *> inside(
          name, {
            (tparams ::: ctor :: template :: early ::: init ::: self :: Nil)
              .traverse_(inspect) *> stats.traverse_(inspect(_))
          }
        )
      case Defn.Object(mods, name, templ @ Template(_, init, _, stats)) =>
        log(s"Object($mods, $name, $templ)")

        define(name) *> link(
          name,
          init.flatMap(treeToNames) ++ stats.flatMap(treeToNames)
        ) *> inside(name, inspect(templ))
      case Defn.Val(mods, pats, optType, term) =>
        log(s"Val($mods, $pats, $optType, $term)")
        List(
          pats.traverse_(definePat),
          optType.traverse_(inspect),
          inspect(term)
        ).sequence_
      case Defn.Var(mods, pats, optType, term) =>
        log(s"Var($mods, $pats, $optType, $term)")
        List(
          pats.traverse_(definePat),
          optType.traverse_(inspect),
          term.traverse_(inspect)
        ).sequence_
      case Defn.Def(mods, name, tparams, paramss, optType, body) =>
        log(s"Defn.Def($mods, $name, $tparams, $paramss, $optType, $body)")
        define(name) *>
          newScope {
            tparams.traverse_ { t =>
              inspect(t) *> define(t.name)
            } *>
              paramss.traverse_ { ps =>
                ps.traverse { t =>
                  inspect(t) *> define(t.name)
                }
              } *>
              List(optType.traverse_(inspect), inspect(body)).sequence_
          }
      case Defn.Trait(
            mods,
            name,
            tparams,
            ctor,
            templ @ Template(_, init, _, stats)
          ) =>
        log(s"Defn.Trait($mods, $name, $tparams, $ctor, $templ)")
        define(name) *> link(
          name,
          init.map(_.tpe).flatMap(typeToNames) ++ stats.flatMap(treeToNames)
        ) *>
          inside(
            name, {
              tparams.traverse_ { t =>
                define(t.name) *> inspect(t)
              } *>
                List(ctor, templ).traverse_(inspect)
            }
          )
      case Defn.Macro(mods, name, tparams, paramss, decltpe, body) =>
        log(s"Defn.Macro($mods, $name, $tparams, $paramss, $decltpe, $body)")
        define(name) *>
          newScope {
            tparams.traverse_ { t =>
              inspect(t) *> define(t.name)
            } *>
              paramss.traverse_ { ps =>
                ps.traverse { t =>
                  inspect(t) *> define(t.name)
                }
              } *>
              List(decltpe.traverse_(inspect(_)), inspect(body)).sequence_
          }
      case Defn.Type(mods, name, tparams, body) =>
        // type Foo[args] = ...
        log(s"Defn.Type($mods, $name, $tparams, $body)")
        define(name) *> (tparams ::: body :: Nil).traverse_(inspect)
      case Self(name, optType) =>
        log(s"Self($name, $optType)")
        // TODO, what is Self?
        (name :: optType.toList).traverse_(inspect)
      case m: Mod =>
        m match {
          case Mod.Annot(init) =>
            log(s"Mod.Annot($init)")
          case Mod.Sealed() =>
            log("sealed")
          case Mod.Private(k) =>
            log(s"Private($k)")
          case Mod.Protected(k) =>
            log(s"Protected($k)")
          case Mod.Abstract() =>
            log("abstract")
          case Mod.Lazy() =>
            log("lazy")
          case Mod.Case() =>
            log("case")
          case Mod.Covariant() =>
            log("covariant")
          case Mod.Contravariant() =>
            log("contravariant")
          case Mod.Final() =>
            log("final")
          case Mod.Implicit() =>
            log("implicit")
          case Mod.Override() =>
            log("override")
          case Mod.ValParam() =>
            log("val param")
          case Mod.VarParam() =>
            log("var param")
        }
        unitEnv
      case Ctor.Primary(mods, name, paramss) =>
        log(s"Ctor.Primary($mods, $name, $paramss)")
        // TODO do we need to process the name?
        paramss.traverse_ { params =>
          params.traverse_ { t =>
            define(t.name) *> inspect(t)
          }
        }
      case Ctor.Secondary(mods, name, params, init, stats) =>
        log(s"Ctor.Secondary($mods, $name, $params, $init, $stats)")
        // TODO we need to know that the params are in scope in the body
        unitEnv
      case l: Lit =>
        l match {
          case Lit.String(str) =>
            log(s"Lit.String($str)")
          case Lit.Symbol(sym) =>
            log(s"Lit.Symbol($sym)")
          case Lit.Int(i) =>
            log(s"Lit.Int($i)")
          case Lit.Long(l) =>
            log(s"Lit.Long($l)")
          case Lit.Double(d) =>
            log(s"Lit.Double($d)")
          case Lit.Float(f) =>
            log(s"Lit.Float($f)")
          case Lit.Boolean(b) =>
            log(s"Lit.Boolean($b)")
          case Lit.Char(c) =>
            log(s"Lit.Char($c)")
          case Lit.Null() =>
            log("Lit.Null()")
          case Lit.Unit() =>
            log("Lit.Unit()")
        }
        unitEnv
      case Pat.Alternative(left, right) =>
        // Foo | Bar
        log(s"Pat.Alernative($left, $right)")
        List(left, right).traverse_(inspect)
      case Pat.Bind(left, pat) =>
        log(s"Pat.Bind($left, $pat)")
        left match {
          case Pat.Var(n) =>
            // foo @ bar
            define(n) *> inspect(pat)
          case other =>
            sys.error(s"unexpected: $other in $tree")
        }
      case Pat.Interpolate(term, parts, args) =>
        log(s"Pat.Interpolate($term, $parts, $args)")
        // q"foo$bar"
        // in a case match
        (term :: args).traverse_(inspect)
      case Pat.Var(name) =>
        log(s"Pat.Var($name)")
        referTo(name)
        // this is not a reference, but it may be a scoped define
        unitEnv
      case Pat.Tuple(items) =>
        log(s"Pat.Tuple($items)")
        items.traverse_(inspect)
      case Pat.Typed(pat, tpe) =>
        // pat: tpe
        log(s"Pat.Typed($pat, $tpe)")
        List(pat, tpe).traverse_(inspect)
      case Pat.Wildcard() =>
        log(s"Pat.Wildcard()")
        unitEnv
      case Pat.Extract(term, args) =>
        log(s"Pat.Extract($term, $args)")
        (term :: args).traverse_(inspect)
      case Pat.ExtractInfix(term, op, args) =>
        log(s"Pat.ExtractInfix($term, $op, $args)")
        referTo(op) *> (term :: args).traverse_(inspect)
      case Pat.SeqWildcard() =>
        log("Pat.SeqWildcard()")
        unitEnv
      case t =>
        sys.error(s"unknown: ${t.getClass} $t")
    }
  }
}
