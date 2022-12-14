package io.bazeltools.buildgen.javadefref

import cats.effect.IO
import com.github.javaparser.ast
import scala.collection.mutable.{Set => MSet, Map => MMap}
import scala.collection.immutable.SortedSet

import scala.jdk.OptionConverters._
import scala.jdk.CollectionConverters._

import ast.CompilationUnit
import ast.`type`.{Type => JType}
import io.bazeltools.buildgen.shared.{Entity, DataBlock}
import cats.data.Chain
import com.github.javaparser.JavaParser
import com.github.javaparser.ParserConfiguration
import com.github.javaparser.ParseProblemException

/*
 * based on Apache Licensed:
 * https://github.com/pantsbuild/pants/blob/4e7c57db992150b3fc972e684561edb8231bba3d/src/python/pants/backend/java/dependency_inference/PantsJavaParserLauncher.java#L1
 */
object JavaSourceEntityExtractor {
  private[this] lazy val parser = {
    val config = new ParserConfiguration();

    config.setLexicalPreservationEnabled(false)
    config.setLanguageLevel(ParserConfiguration.LanguageLevel.RAW)
    new JavaParser(config)
  }

  def extract(content: String): IO[DataBlock] = {
    val result = parser.parse(content)
    (if (result.isSuccessful()) {
       IO.pure(result.getResult().get())
     } else {
       IO.raiseError(new ParseProblemException(result.getProblems()))
     }).map(structureOf(_))
  }

  def structureOf(compUnit: CompilationUnit): DataBlock = {
    import Entity._
    // The parser is imperative and mutable, so we take a non-idiomatic
    // approach here and use mutable values to keep state

    val optPack: Option[Entity] =
      compUnit.getPackageDeclaration.toScala.map { p =>
        Entity.dotted(p.getName().toString)
      }

    val (wildImp, fixedImp) =
      compUnit.getImports().asScala.to(LazyList).partitionMap { i =>
        val e = Entity.dotted(i.getName.toString)
        if (i.isAsterisk()) {
          Left(e)
        } else if (i.isStatic) {
          Right(e.init)
        } else Right(e)
      }

    // TODO there needs to be some way to configure this
    val generators: Set[String] =
      Set("InputTransformDef", "FeatureEncoderDef", "DataTransformDef")

    val topLevelDefsTypes: SortedSet[Entity] =
      compUnit
        .getTypes()
        .iterator
        .asScala
        .flatMap { t =>
          t.getFullyQualifiedName().toScala.toList.flatMap { fqn =>
            import com.github.javaparser.ast.expr.SingleMemberAnnotationExpr
            val anns = t.getAnnotations
            val generated = anns.asScala.toList.flatMap {
              case sc: SingleMemberAnnotationExpr
                  if generators(sc.getName.asString) =>
                // these drop the Def off the end and generate that type
                val nonDef = if (fqn.endsWith("Def")) {
                  fqn.dropRight(3)
                } else {
                  fqn
                }
                val res = Entity.dotted(nonDef) :: Nil
                // System.err.println(s"generating: $res")
                res
              case _ =>
                // System.err.println(s"ignored annotation: $other")
                Nil
            }

            Entity.dotted(fqn) :: generated
          }
        }
        .to(SortedSet)

    val typeCache: MMap[JType, LazyList[Entity]] = MMap()

    def jtypeToEntities(jt: JType): LazyList[Entity] =
      typeCache.getOrElseUpdate(
        jt,
        if (jt.isArrayType())
          jtypeToEntities(jt.asArrayType().getComponentType())
        else if (jt.isWildcardType()) {
          val wildcardType = jt.asWildcardType()
          wildcardType
            .getExtendedType()
            .toScala
            .to(LazyList)
            .flatMap(jtypeToEntities(_)) #:::
            wildcardType
              .getSuperType()
              .toScala
              .to(LazyList)
              .flatMap(jtypeToEntities(_))
        } else if (jt.isClassOrInterfaceType()) {
          val classType = jt.asClassOrInterfaceType();
          Entity.dotted(classType.getNameWithScope()) #::
            classType.getTypeArguments.toScala
              .to(LazyList)
              .flatMap { ts => ts.asScala }
              .flatMap(jtypeToEntities(_))

        } else if (jt.isIntersectionType()) {
          jt.asIntersectionType()
            .getElements()
            .asScala
            .to(LazyList)
            .flatMap(jtypeToEntities(_))
        } else LazyList.empty
      )

    var allDirectives: Chain[String] =
      Chain.empty

    val (referencedTypes: Set[JType], refNames: Set[String]) = {
      import ast.Node
      import ast.nodeTypes.NodeWithType
      import ast.body.{ClassOrInterfaceDeclaration, MethodDeclaration}
      import ast.expr.{
        AnnotationExpr,
        MethodCallExpr,
        NameExpr,
        FieldAccessExpr
      }
      import ast.comments.Comment

      val refTypes: MSet[JType] = MSet()
      val names: MSet[String] = MSet()

      def process(n: Node): Unit = {
        n match {
          case md: MethodDeclaration =>
            refTypes += md.getType
            md.getParameters.iterator().asScala.foreach { p =>
              refTypes += p.getType
            }
            md.getThrownExceptions().iterator.asScala.foreach { e =>
              refTypes += e
            }
          case cid: ClassOrInterfaceDeclaration =>
            refTypes ++= cid.getExtendedTypes().asScala
            refTypes ++= cid.getImplementedTypes().asScala
          case an: AnnotationExpr =>
            names += an.getNameAsString()
          case compilationUnit: CompilationUnit =>
            compilationUnit.getAllComments().asScala.foreach { c =>
              Entity.findDirectives(c.getContent()) match {
                case Right(ds) =>
                  allDirectives = allDirectives ++ ds
                case Left(err) =>
                  sys.error(
                    s"couldn't parse:\n${c.getContent()}\n-------------\n$err"
                  )
              }
            }
          case mc: MethodCallExpr =>
            // a.foo(b)
            // then a is the scope
            // we need to track to see if we are in a method
            // and that name is private, for now we assume not
            mc.getScope.toScala match {
              case Some(nx: NameExpr) =>
                names += nx.getNameAsString()
              case _ =>
                ()
            }
            mc.getArguments.asScala.foreach(process(_))
          case fa: FieldAccessExpr =>
            fa.getScope match {
              case nx: NameExpr =>
                names += nx.getNameAsString()
              case notName =>
                process(notName)
            }
          case c: Comment =>
            Entity.findDirectives(c.getContent()) match {
              case Right(ds) =>
                allDirectives = allDirectives ++ ds
              case Left(err) =>
                sys.error(
                  s"couldn't parse:\n${c.getContent()}\n-------------\n$err"
                )
            }
          case nt: NodeWithType[_, _] =>
            // this is an abstract class so should be at the end
            refTypes += nt.getType
          case _ =>
            // TODO add more
            // println(s"unknown: ${n.getClass}: $n")
            ()
        }

      }

      compUnit.walk(process(_))

      (refTypes.to(Set), names.to(Set))
    }

    /*
     * These are all the possible "root" references.
     * for names like Foo, they could be:
         1. in our package,
         2. in any wildcard import
         3. in the root of the namespace
     */
    val rootRefs: LazyList[Entity] =
      referencedTypes.to(LazyList).flatMap(jtypeToEntities(_)) #:::
        refNames.to(LazyList).map(Entity.dotted(_))

    val expand: Entity => LazyList[Entity] =
      optPack match {
        case Some(p) => { (e: Entity) =>
          if (e.isSingleton) {
            e #:: (p / e) #:: wildImp.map(_ / e)
          } else e #:: LazyList.empty
        }
        case None => { (e: Entity) =>
          if (e.isSingleton) {
            e #:: wildImp.map(_ / e)
          } else e #:: LazyList.empty
        }
      }

    val refs =
      (rootRefs.flatMap(expand) #::: fixedImp #::: wildImp).to(SortedSet)

    allDirectives.foldLeft(DataBlock("", topLevelDefsTypes, refs)) {
      case (prev, n) => prev.addBzlBuildGenCommand(n)
    }
  }

}
